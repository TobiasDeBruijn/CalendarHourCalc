use std::env;
use std::path::Path;
use tokio::fs;
use color_eyre::Result;
use itext::itext::kernel::{PdfDocument, PdfWriter};
use itext::itext::layout::{BlockElement, Border, Cell, Document, ElementPropertyContainer, HorizontalAlignment, Paragraph, Table, TextAlignment};
use itext::java::ByteArrayOutputStream;
use jni::{InitArgsBuilder, JavaVM, JNIEnv, JNIVersion};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::task::block_in_place;
use crate::{calc_total_duration, EventSummary, fmt_duration};

/// Java VM with jarfile dependencies
struct DependentJavaVM {
    /// The VM itself
    javavm: JavaVM,
    /// Directory containing jarfile dependencies
    // Kept so that the tempdir continues to exist while
    // the JVM is running
    _tempdir: TempDir,
}

impl DependentJavaVM {
    /// Create a new JVM with jarfile dependencies
    ///
    /// # Errors
    ///
    /// - If saving a Jarfile to disk failed
    /// - If creating the initialization arguments failed
    /// - If creating the JVM failed
    pub async fn new() -> Result<Self> {
        let tempdir = TempDir::new()?;
        let classpath = vec![
            Self::write_jar(tracing_slf4j::DEPENDENCIES, "tracing_slf4j.jar", tempdir.path()).await?,
            Self::write_jar(itext::bundle::DEPENDENCIES, "itext.jar", tempdir.path()).await?,
        ];

        let args = InitArgsBuilder::new()
            .version(JNIVersion::V8)
            .option(format!("-Djava.class.path={}", classpath.join(":")))
            //.option("-Xcheck:jni")
            //.option("-verbose:jni")
            .build()?;
        let javavm = JavaVM::new(args)?;

        Ok(Self {
            javavm,
            _tempdir: tempdir
        })
    }

    /// Write a jarfile to disk
    ///
    /// # Errors
    ///
    /// if an IO error occurred
    async fn write_jar(bytes: &[u8], name: &str, dir: &Path) -> Result<String> {
        let path = dir.join(name);
        let mut kernel = fs::File::create(&path).await?;
        kernel.write_all(bytes).await?;

        Ok(path.to_string_lossy().to_string())
    }
}

pub async fn generate_pdf(name: &str, events: &[EventSummary]) -> Result<()> {
    let jvm = DependentJavaVM::new().await?;
    let bytes = block_in_place(move || generate_pdf_inner(jvm, name, events))?;
    let output_path = env::current_dir()?.join(format!("{name}.pdf"));
    let mut file = fs::File::create(output_path).await?;
    file.write_all(&bytes).await?;

    Ok(())
}

fn generate_pdf_inner(jvm: DependentJavaVM, name: &str, events: &[EventSummary]) -> Result<Vec<u8>> {
    let mut env = jvm.javavm.attach_current_thread()?;

    tracing_slf4j::register_log_fn(&mut env)?;

    let byte_out = ByteArrayOutputStream::new(&mut env)?;
    let pdf_writer = PdfWriter::new(&byte_out, &mut env)?;
    let pdf_document = PdfDocument::new(&pdf_writer, &mut env)?;
    let doc = Document::new(&pdf_document, &mut env)?;

    doc.set_margins(40.0, 30.0, 40.0, 30.0, &mut env)?;

    let header_table = Table::new(&[2.0, 2.0], &mut env)?;

    // Document header

    let cell = Cell::new(&mut env)?;
    cell.add_paragraph(Paragraph::new_with_text("Urenregistratie", &mut env)?, &mut env)?;
    cell.set_bold(&mut env)?;
    cell.set_border(Border::NoBorder, &mut env)?;
    cell.set_text_alignment(TextAlignment::Left, &mut env)?;

    header_table.start_new_row(&mut env)?;
    header_table.add_cell(cell, &mut env)?;
    header_table.add_cell(get_empty_cell(Border::NoBorder, 24.0, &mut env)?, &mut env)?;

    header_table.start_new_row(&mut env)?;
    header_table.add_cell(get_cell("Bedrijf:", Border::NoBorder, &mut env)?, &mut env)?;
    header_table.add_cell(get_cell(name, Border::NoBorder, &mut env)?, &mut env)?;

    doc.add_table(header_table, &mut env)?;

    // Document content

    let hour_table = Table::new(&[2.0, 2.0, 2.0], &mut env)?;
    hour_table.set_horizontal_alignment(HorizontalAlignment::Center, &mut env)?;
    hour_table.use_all_available_width(&mut env)?;

    // Headers
    hour_table.start_new_row(&mut env)?;
    let cell = Cell::new(&mut env)?;
    cell.add_paragraph(Paragraph::new_with_text("Datum", &mut env)?, &mut env)?;
    cell.set_bold(&mut env)?;
    cell.set_border(Border::NoBorder, &mut env)?;
    hour_table.add_cell(cell, &mut env)?;

    let cell = Cell::new(&mut env)?;
    cell.add_paragraph(Paragraph::new_with_text("Tijd", &mut env)?, &mut env)?;
    cell.set_bold(&mut env)?;
    cell.set_border(Border::NoBorder, &mut env)?;
    hour_table.add_cell(cell, &mut env)?;

    let cell = Cell::new(&mut env)?;
    cell.add_paragraph(Paragraph::new_with_text("Duratie", &mut env)?, &mut env)?;
    cell.set_bold(&mut env)?;
    cell.set_border(Border::NoBorder, &mut env)?;
    hour_table.add_cell(cell, &mut env)?;

    for event in events {
        hour_table.start_new_row(&mut env)?;
        hour_table.add_cell(get_cell(&event.date, Border::NoBorder, &mut env)?, &mut env)?;
        hour_table.add_cell(get_cell(&event.time, Border::NoBorder, &mut env)?, &mut env)?;
        hour_table.add_cell(get_cell(&event.duration, Border::NoBorder, &mut env)?, &mut env)?;
    }

    // Empty row
    hour_table.start_new_row(&mut env)?;
    hour_table.add_cell(get_empty_cell(Border::NoBorder, 24.0, &mut env)?, &mut env)?;

    // Totals
    hour_table.start_new_row(&mut env)?;
    hour_table.add_cell(get_empty_cell(Border::NoBorder, 24.0, &mut env)?, &mut env)?;
    hour_table.add_cell(get_cell("Totaal", Border::NoBorder, &mut env)?, &mut env)?;
    hour_table.add_cell(get_cell(&fmt_duration(calc_total_duration(events)), Border::NoBorder, &mut env)?, &mut env)?;

    doc.add_table(hour_table, &mut env)?;

    // Export document

    doc.close(&mut env)?;
    let bytes = byte_out.to_byte_array(&mut env)?;

    Ok(bytes)
}

fn get_empty_cell<'a>(border: Border, height: f32, env: &mut JNIEnv<'a>) -> Result<Cell<'a>> {
    let cell = Cell::new(env)?;
    cell.set_border(border, env)?;

    let paragraph = Paragraph::new(env)?;
    paragraph.set_height(height, env)?;
    cell.add_paragraph(paragraph, env)?;

    Ok(cell)
}

fn get_cell<'a>(text: &str, border: Border, env: &mut JNIEnv<'a>) -> Result<Cell<'a>> {
    let cell = Cell::new(env)?;
    let paragraph = Paragraph::new_with_text(text, env)?;
    cell.add_paragraph(paragraph, env)?;
    cell.set_border(border, env)?;

    Ok(cell)
}