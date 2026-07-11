use printpdf::{BuiltinFont, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference};
use std::io::BufWriter;

use crate::{error::AppError, invoicing::InvoiceSummary};

fn write_line(
    layer: &PdfLayerReference,
    font: &printpdf::IndirectFontRef,
    y_mm: f32,
    text: &str,
) {
    layer.use_text(text, 11.0, Mm(20.0), Mm(y_mm), font);
}

pub fn render_invoice_pdf(
    invoice: &InvoiceSummary,
    business_name: &str,
) -> Result<Vec<u8>, AppError> {
    let title = format!(
        "Invoice {}",
        invoice
            .invoice_number
            .as_deref()
            .unwrap_or("draft")
    );
    let (doc, page1, layer1) = PdfDocument::new(&title, Mm(210.0), Mm(297.0), "Layer 1");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|error| AppError::internal(error.to_string()))?;
    let layer = doc.get_page(page1).get_layer(layer1);

    let mut y = 270.0_f32;
    write_line(&layer, &font, y, "ÖppenBokföring");
    y -= 8.0;
    write_line(&layer, &font, y, business_name);
    y -= 14.0;
    write_line(&layer, &font, y, &title);
    y -= 10.0;
    write_line(
        &layer,
        &font,
        y,
        &format!("Customer: {}", invoice.counterparty_name),
    );
    y -= 8.0;
    if let Some(issue_date) = &invoice.issue_date {
        write_line(&layer, &font, y, &format!("Issue date: {issue_date}"));
        y -= 8.0;
    }
    if let Some(due_date) = &invoice.due_date {
        write_line(&layer, &font, y, &format!("Due date: {due_date}"));
        y -= 12.0;
    }

    write_line(&layer, &font, y, "Description");
    write_line(&layer, &font, y, "Ex VAT");
    y -= 8.0;
    for line in &invoice.lines {
        write_line(
            &layer,
            &font,
            y,
            &format!(
                "{} x {} — {} SEK",
                line.quantity,
                line.description,
                format_minor(line.line_ex_vat_minor)
            ),
        );
        y -= 7.0;
        if y < 30.0 {
            break;
        }
    }

    y -= 6.0;
    write_line(
        &layer,
        &font,
        y,
        &format!("Total ex VAT: {} SEK", format_minor(invoice.total_ex_vat_minor)),
    );
    y -= 8.0;
    write_line(
        &layer,
        &font,
        y,
        &format!("VAT: {} SEK", format_minor(invoice.total_vat_minor)),
    );
    y -= 8.0;
    write_line(
        &layer,
        &font,
        y,
        &format!(
            "Total inc VAT: {} SEK",
            format_minor(invoice.total_inc_vat_minor)
        ),
    );

    save_document(doc)
}

fn format_minor(minor: i64) -> String {
    let whole = minor / 100;
    let frac = (minor.abs() % 100) as i64;
    format!("{whole}.{frac:02}")
}

fn save_document(doc: PdfDocumentReference) -> Result<Vec<u8>, AppError> {
    let mut buffer = Vec::new();
    doc.save(&mut BufWriter::new(&mut buffer))
        .map_err(|error| AppError::internal(error.to_string()))?;
    Ok(buffer)
}
