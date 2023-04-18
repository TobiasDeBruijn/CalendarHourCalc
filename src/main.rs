use crate::args::{Args, Commands, ConfigureCommands, IcsCommands};
use chrono::{DateTime, Datelike, Timelike};
use clap::Parser;
use color_eyre::eyre::{Error, Result};
use ical::IcalParser;
use std::io::{BufReader, Cursor};
use reqwest::Client;
use tabled::{Panel, Style, Table, Tabled};
use tracing::warn;
use crate::config::Config;

mod args;
mod config;

#[derive(Tabled)]
struct EventSummary {
    #[tabled(rename = "Date")]
    date: String,
    #[tabled(rename = "Time")]
    time: String,
    #[tabled(rename = "Duration")]
    duration: String,
    #[tabled(skip)]
    date_start: u32,
    #[tabled(skip)]
    month_start: u32,
    #[tabled(skip)]
    year_start: i32,
    #[tabled(skip)]
    duration_sec: i64,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Args::parse();

    let mut config = Config::open().await?.unwrap_or_default();

    match args.commands {
        Commands::Configure { configure_commands } => match configure_commands {
            ConfigureCommands::Ics { ics_commands } => match ics_commands {
                IcsCommands::List => ics_list(&mut config).await?,
                IcsCommands::Add { link } => ics_add(&mut config, link).await?,
                IcsCommands::Remove { index } => ics_remove(&mut config, index).await?,
            }
        },
        Commands::Report { ics_index, month, year } => report(&mut config, ics_index, month, year).await?
    };

    Ok(())
}

async fn ics_list(config: &mut Config) -> Result<()> {
    #[derive(Tabled)]
    struct IcsList<'a> {
        #[tabled(rename = "Index")]
        index: usize,
        #[tabled(rename = "URL")]
        url: &'a str,
    }

    let ics = config.ical.iter()
        .enumerate()
        .map(|(index, url)| IcsList {
            index,
            url
        })
        .collect::<Vec<_>>();

    let table = Table::new(ics.iter())
        .with(Style::rounded())
        .to_string();
    println!("{table}");
    Ok(())
}

async fn ics_add(config: &mut Config, link: String) -> Result<()> {
    if config.ical.contains(&link) {
        return Err(Error::msg("Already exists"));
    }

    config.ical.push(link);
    config.store().await
}

async fn ics_remove(config: &mut Config, index: usize) -> Result<()> {
    if index >= config.ical.len() {
        return Err(Error::msg("Invalid index"));
    }

    config.ical.remove(index);
    config.store().await
}

async fn report(config: &mut Config, ics_index: usize, month: Option<u32>, year: Option<i32>) -> Result<()> {
    let ics_url = config.ical.get(ics_index)
        .ok_or(Error::msg("Invalid index"))?;
    let parser = download_ical(ics_url).await?;

    // An ics file can contain multiple calendars, we just sum them up
    let events = parser
        .into_iter()
        .map(|ical| {
            let ical = ical?;

            // Sum up every event in the calendar
            let event_summaries = ical
                .events
                .iter()
                .map(|event| {
                    // Get the start property
                    let dtstart = event.properties.iter().find(|prop| prop.name.eq("DTSTART"));

                    let dtstart = match dtstart {
                        Some(x) if x.value.is_some() => x.value.clone().unwrap(),
                        Some(_) | None => {
                            warn!("Event is missing start property, skipping!");
                            return Ok(None);
                        }
                    };

                    // Get the end property
                    let dtend = event.properties.iter().find(|prop| prop.name.eq("DTEND"));
                    let dtend = match dtend {
                        Some(x) if x.value.is_some() => x.value.clone().unwrap(),
                        Some(_) | None => {
                            warn!("Event is missing end property, skipping!");
                            return Ok(None);
                        }
                    };

                    // Convert both to DateTime
                    let start = hypentate_dttime(&dtstart);
                    let start = DateTime::parse_from_rfc3339(&start)?;

                    let end = hypentate_dttime(&dtend);
                    let end = DateTime::parse_from_rfc3339(&end)?;

                    // Format the event date as DD-MM-YYYY - DD-MM-YYYY
                    // Account for if the date spans multiple days
                    let date = if start.day() == end.day() {
                        format!("{:02}-{:02}-{}", start.day(), start.month(), start.year())
                    } else {
                        format!(
                            "{:02}-{:02}-{} - {:02}-{:02}-{}",
                            start.day(),
                            start.month(),
                            start.year(),
                            end.day(),
                            end.month(),
                            end.year()
                        )
                    };

                    // Format the event timespan as HH:MM:SS - HH:MM:SS
                    let time = format!(
                        "{:02}:{:02} - {:02}:{:02}",
                        start.hour(),
                        start.minute(),
                        end.hour(),
                        end.minute()
                    );

                    let duration = end - start;
                    Ok(Some(EventSummary {
                        date,
                        time,
                        duration: fmt_duration(duration.num_seconds()),
                        duration_sec: duration.num_seconds(),
                        date_start: start.day(),
                        month_start: start.month(),
                        year_start: start.year(),
                    }))
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .filter_map(|x| x)
                .collect::<Vec<_>>();
            Ok(event_summaries)
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let mut events = events
        .into_iter()
        .filter(|event| month.map(|month| event.month_start == month).unwrap_or(true))
        .filter(|event| year.map(|year| event.year_start == year).unwrap_or(true))
        .collect::<Vec<_>>();

    // Sort by date
    events.sort_by(|a, b| a.date_start.cmp(&b.date_start));

    // Calculate the summed duration of all events
    let total_duration: i64 = events
        .iter()
        .map(|x| x.duration_sec)
        .sum();

    // Pretty-print as a table
    // Adding an empty row and a footer at the bottom
    // to display the total time
    let table = Table::new(events.iter())
        .with(Style::rounded())
        .with(Panel::horizontal(events.len() + 1).column(2))
        .with(Panel::horizontal(events.len() + 2).column(2).text(format!(
            "Total: {} (HH:MM:SS)",
            fmt_duration(total_duration)
        )))
        .to_string();

    println!("{table}");
    Ok(())
}

async fn download_ical(url: &str) -> Result<IcalParser<BufReader<Cursor<Vec<u8>>>>> {
    let body_bytes = Client::new()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();

    Ok(IcalParser::new(BufReader::new(Cursor::new(body_bytes))))
}

/// Format a duration in seconds as HH:MM:SS
fn fmt_duration(secs: i64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        (secs / 60) / 60,
        (secs / 60) % 60,
        secs % 60
    )
}

/// Insert hyphens and colons into the dttime string
/// E.g 20220921T151530Z will become 2022-09-21T15:15:30Z
fn hypentate_dttime(input: &str) -> String {
    let mut buf = String::new();
    for (idx, char) in input.chars().enumerate() {
        buf.push(char);

        if idx == 3 || idx == 5 {
            buf.push('-');
        }

        if idx == 10 || idx == 12 {
            buf.push(':');
        }
    }

    buf
}
