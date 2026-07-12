use printpdf::{
    path::PaintMode, BuiltinFont, Color, Greyscale, Line, Mm, PdfDocument, PdfDocumentReference,
    PdfLayerReference, Point, Rect,
};
use std::io::BufWriter;

use crate::{error::{AppError, redacted_internal_from}, invoicing::InvoiceSummary};

#[derive(Debug, Clone)]
pub struct InvoicePdfContext {
    pub business_name: String,
    pub owner_name: String,
    pub tax_status: String,
    pub vat_status: String,
}

const PAGE_HEIGHT_MM: f32 = 297.0;
const MARGIN_LEFT_MM: f32 = 15.0;
const MARGIN_RIGHT_MM: f32 = 15.0;
const PAGE_WIDTH_MM: f32 = 210.0;
const CONTENT_WIDTH_MM: f32 = PAGE_WIDTH_MM - MARGIN_LEFT_MM - MARGIN_RIGHT_MM;
const BODY_FONT_PT: f32 = 10.0;
const TITLE_FONT_PT: f32 = 14.0;
const ROW_HEIGHT_MM: f32 = 9.0;
const CELL_PAD_X_MM: f32 = 2.0;
const TEXT_BASELINE_OFFSET_MM: f32 = 3.2;

/// Text fill color must be reset to this before every `use_text` call.
/// `set_fill_color` is shared graphics state between shape fills (e.g. row
/// shading) and text rendering in printpdf, so a grey rect fill left in
/// place would otherwise make subsequently drawn text almost invisible.
fn black_fill() -> Color {
    Color::Greyscale(Greyscale::new(0.0, None))
}

fn set_text_color_black(layer: &PdfLayerReference) {
    layer.set_fill_color(black_fill());
}

#[derive(Clone, Copy)]
enum TextAlign {
    Left,
    Center,
    Right,
}

struct GridTable {
    left_mm: f32,
    top_mm: f32,
    col_widths_mm: Vec<f32>,
    row_height_mm: f32,
}

impl GridTable {
    fn new(left_mm: f32, top_mm: f32, col_widths_mm: Vec<f32>) -> Self {
        Self {
            left_mm,
            top_mm,
            col_widths_mm,
            row_height_mm: ROW_HEIGHT_MM,
        }
    }

    fn width_mm(&self) -> f32 {
        self.col_widths_mm.iter().sum()
    }

    fn col_left_mm(&self, column: usize) -> f32 {
        self.left_mm + self.col_widths_mm[..column].iter().sum::<f32>()
    }

    fn row_top_mm(&self, row: usize) -> f32 {
        self.top_mm + row as f32 * self.row_height_mm
    }

    fn cell_bottom_mm(&self, row: usize) -> f32 {
        PAGE_HEIGHT_MM - self.row_top_mm(row) - self.row_height_mm
    }

    fn cell_top_mm(&self, row: usize) -> f32 {
        PAGE_HEIGHT_MM - self.row_top_mm(row)
    }

    fn text_baseline_mm(&self, row: usize) -> f32 {
        self.cell_bottom_mm(row) + TEXT_BASELINE_OFFSET_MM
    }

    fn fill_row(
        &self,
        layer: &PdfLayerReference,
        row: usize,
        grey: f32,
    ) {
        // Scope the grey fill color to this rect only: printpdf shares the
        // fill color graphics state between shapes and text, so without
        // save/restore the grey would leak into every text draw that
        // follows on the page.
        layer.save_graphics_state();
        layer.set_fill_color(Color::Greyscale(Greyscale::new(grey, None)));
        let rect = Rect::new(
            Mm(self.left_mm),
            Mm(self.cell_bottom_mm(row)),
            Mm(self.left_mm + self.width_mm()),
            Mm(self.cell_top_mm(row)),
        )
        .with_mode(PaintMode::Fill);
        layer.add_rect(rect);
        layer.restore_graphics_state();
    }

    fn stroke_grid(&self, layer: &PdfLayerReference, rows: usize) {
        layer.set_outline_color(Color::Greyscale(Greyscale::new(0.35, None)));
        layer.set_outline_thickness(0.4);

        let height_mm = rows as f32 * self.row_height_mm;
        let bottom_mm = PAGE_HEIGHT_MM - self.top_mm - height_mm;
        let top_mm = PAGE_HEIGHT_MM - self.top_mm;
        let right_mm = self.left_mm + self.width_mm();

        let outline = Line {
            points: vec![
                (Point::new(Mm(self.left_mm), Mm(bottom_mm)), false),
                (Point::new(Mm(right_mm), Mm(bottom_mm)), false),
                (Point::new(Mm(right_mm), Mm(top_mm)), false),
                (Point::new(Mm(self.left_mm), Mm(top_mm)), false),
            ],
            is_closed: true,
        };
        layer.add_line(outline);

        for column in 1..self.col_widths_mm.len() {
            let x = self.col_left_mm(column);
            let vertical = Line {
                points: vec![
                    (Point::new(Mm(x), Mm(bottom_mm)), false),
                    (Point::new(Mm(x), Mm(top_mm)), false),
                ],
                is_closed: false,
            };
            layer.add_line(vertical);
        }

        for row in 1..rows {
            let y = PAGE_HEIGHT_MM - self.row_top_mm(row);
            let horizontal = Line {
                points: vec![
                    (Point::new(Mm(self.left_mm), Mm(y)), false),
                    (Point::new(Mm(right_mm), Mm(y)), false),
                ],
                is_closed: false,
            };
            layer.add_line(horizontal);
        }
    }

    fn write_cell(
        &self,
        layer: &PdfLayerReference,
        font: &printpdf::IndirectFontRef,
        row: usize,
        column: usize,
        text: &str,
        align: TextAlign,
    ) {
        let x_left = self.col_left_mm(column) + CELL_PAD_X_MM;
        let x_right = self.col_left_mm(column) + self.col_widths_mm[column] - CELL_PAD_X_MM;
        let y = self.text_baseline_mm(row);
        let x = match align {
            TextAlign::Left => x_left,
            TextAlign::Center => {
                let width = text_width_mm(text, BODY_FONT_PT);
                x_left + (self.col_widths_mm[column] - 2.0 * CELL_PAD_X_MM - width) / 2.0
            }
            TextAlign::Right => x_right - text_width_mm(text, BODY_FONT_PT),
        };
        set_text_color_black(layer);
        layer.use_text(text, BODY_FONT_PT, Mm(x), Mm(y), font);
    }
}

fn text_width_mm(text: &str, font_size_pt: f32) -> f32 {
    text.chars().count() as f32 * font_size_pt * 0.5 * 0.352_778
}

fn write_text_at(
    layer: &PdfLayerReference,
    font: &printpdf::IndirectFontRef,
    font_size_pt: f32,
    x_mm: f32,
    y_from_top_mm: f32,
    text: &str,
) {
    let y_mm = PAGE_HEIGHT_MM - y_from_top_mm;
    set_text_color_black(layer);
    layer.use_text(text, font_size_pt, Mm(x_mm), Mm(y_mm), font);
}

fn write_text_right_at(
    layer: &PdfLayerReference,
    font: &printpdf::IndirectFontRef,
    font_size_pt: f32,
    right_x_mm: f32,
    y_from_top_mm: f32,
    text: &str,
) {
    let x_mm = right_x_mm - text_width_mm(text, font_size_pt);
    write_text_at(layer, font, font_size_pt, x_mm, y_from_top_mm, text);
}

pub fn format_sek_minor(minor: i64) -> String {
    let negative = minor < 0;
    let abs_minor = minor.abs();
    let whole = abs_minor / 100;
    let frac = abs_minor % 100;
    let whole_str = format_thousands_se(whole);
    let amount = format!("{whole_str},{frac:02} kr");
    if negative {
        format!("-{amount}")
    } else {
        amount
    }
}

fn format_thousands_se(value: i64) -> String {
    let digits: Vec<char> = value.to_string().chars().collect();
    if digits.is_empty() {
        return "0".to_string();
    }
    let mut out = String::new();
    for (index, ch) in digits.iter().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push('\u{00a0}');
        }
        out.push(*ch);
    }
    out
}

fn format_vat_rate_bp(vat_rate_bp: i64) -> String {
    let whole = vat_rate_bp / 100;
    let frac = vat_rate_bp % 100;
    if frac == 0 {
        format!("{whole} %")
    } else {
        format!("{whole},{frac:02} %")
    }
}

fn invoice_title(invoice: &InvoiceSummary) -> String {
    let number = invoice
        .invoice_number
        .as_deref()
        .unwrap_or("utkast");
    if invoice.invoice_kind == "credit_note" {
        format!("KREDITFAKTURA {number}")
    } else {
        format!("FAKTURA {number}")
    }
}

fn compliance_footer_lines(context: &InvoicePdfContext) -> Vec<String> {
    let mut lines = Vec::new();
    if matches!(context.tax_status.as_str(), "f_skatt" | "fa_skatt") {
        lines.push("Godkänd för F-skatt".to_string());
    }
    if context.vat_status == "exempt_low_turnover" {
        lines.push(
            "Momsbefriad enligt 9 kap. 51 § mervärdesskattelagen (moms 0 %).".to_string(),
        );
    }
    lines
}

fn draw_header(
    layer: &PdfLayerReference,
    font: &printpdf::IndirectFontRef,
    font_bold: &printpdf::IndirectFontRef,
    business_name: &str,
    title: &str,
    issue_date: Option<&str>,
    due_date: Option<&str>,
) -> f32 {
    let top = 20.0;
    write_text_at(
        layer,
        font_bold,
        TITLE_FONT_PT,
        MARGIN_LEFT_MM,
        top,
        business_name,
    );

    let right_edge = MARGIN_LEFT_MM + CONTENT_WIDTH_MM;
    write_text_right_at(layer, font_bold, TITLE_FONT_PT, right_edge, top, title);

    let mut meta_top = top + 10.0;
    if let Some(issue_date) = issue_date {
        write_text_right_at(
            layer,
            font,
            BODY_FONT_PT,
            right_edge,
            meta_top,
            &format!("Fakturadatum: {issue_date}"),
        );
        meta_top += 6.0;
    }
    if let Some(due_date) = due_date {
        write_text_right_at(
            layer,
            font,
            BODY_FONT_PT,
            right_edge,
            meta_top,
            &format!("Förfallodatum: {due_date}"),
        );
        meta_top += 6.0;
    }

    meta_top.max(top + 16.0) + 8.0
}

fn draw_customer_table(
    layer: &PdfLayerReference,
    font: &printpdf::IndirectFontRef,
    font_bold: &printpdf::IndirectFontRef,
    top_mm: f32,
    customer_name: &str,
) -> f32 {
    let table = GridTable::new(MARGIN_LEFT_MM, top_mm, vec![28.0, CONTENT_WIDTH_MM - 28.0]);
    table.fill_row(layer, 0, 0.94);
    table.stroke_grid(layer, 1);
    table.write_cell(layer, font_bold, 0, 0, "Kund", TextAlign::Left);
    table.write_cell(layer, font, 0, 1, customer_name, TextAlign::Left);
    top_mm + ROW_HEIGHT_MM + 10.0
}

fn draw_line_items_table(
    layer: &PdfLayerReference,
    font: &printpdf::IndirectFontRef,
    font_bold: &printpdf::IndirectFontRef,
    top_mm: f32,
    invoice: &InvoiceSummary,
) -> f32 {
    let col_widths = vec![78.0, 15.0, 32.0, 20.0, CONTENT_WIDTH_MM - 145.0];
    let line_rows = invoice.lines.len().max(1);
    let rows = line_rows + 1;
    let table = GridTable::new(MARGIN_LEFT_MM, top_mm, col_widths);

    table.fill_row(layer, 0, 0.92);
    table.stroke_grid(layer, rows);

    let headers = [
        "Beskrivning",
        "Antal",
        "á pris exkl. moms",
        "Moms",
        "Belopp exkl. moms",
    ];
    for (column, header) in headers.iter().enumerate() {
        let align = match column {
            0 => TextAlign::Left,
            1 => TextAlign::Center,
            _ => TextAlign::Right,
        };
        table.write_cell(layer, font_bold, 0, column, header, align);
    }

    if invoice.lines.is_empty() {
        table.write_cell(layer, font, 1, 0, "—", TextAlign::Left);
    } else {
        for (index, line) in invoice.lines.iter().enumerate() {
            let row = index + 1;
            table.write_cell(layer, font, row, 0, &line.description, TextAlign::Left);
            table.write_cell(
                layer,
                font,
                row,
                1,
                &line.quantity.to_string(),
                TextAlign::Center,
            );
            table.write_cell(
                layer,
                font,
                row,
                2,
                &format_sek_minor(line.unit_price_minor),
                TextAlign::Right,
            );
            table.write_cell(
                layer,
                font,
                row,
                3,
                &format_vat_rate_bp(line.vat_rate_bp),
                TextAlign::Right,
            );
            table.write_cell(
                layer,
                font,
                row,
                4,
                &format_sek_minor(line.line_ex_vat_minor),
                TextAlign::Right,
            );
        }
    }

    top_mm + rows as f32 * ROW_HEIGHT_MM + 8.0
}

fn draw_totals_table(
    layer: &PdfLayerReference,
    font: &printpdf::IndirectFontRef,
    font_bold: &printpdf::IndirectFontRef,
    top_mm: f32,
    invoice: &InvoiceSummary,
) -> f32 {
    let table_width = 88.0;
    let left = MARGIN_LEFT_MM + CONTENT_WIDTH_MM - table_width;
    let table = GridTable::new(left, top_mm, vec![48.0, table_width - 48.0]);
    let rows = 3;

    table.fill_row(layer, 2, 0.94);
    table.stroke_grid(layer, rows);
    table.write_cell(
        layer,
        font,
        0,
        0,
        "Summa exkl. moms",
        TextAlign::Left,
    );
    table.write_cell(
        layer,
        font,
        0,
        1,
        &format_sek_minor(invoice.total_ex_vat_minor),
        TextAlign::Right,
    );
    table.write_cell(layer, font, 1, 0, "Moms", TextAlign::Left);
    table.write_cell(
        layer,
        font,
        1,
        1,
        &format_sek_minor(invoice.total_vat_minor),
        TextAlign::Right,
    );
    table.write_cell(layer, font_bold, 2, 0, "Att betala", TextAlign::Left);
    table.write_cell(
        layer,
        font_bold,
        2,
        1,
        &format_sek_minor(invoice.total_inc_vat_minor),
        TextAlign::Right,
    );

    top_mm + rows as f32 * ROW_HEIGHT_MM + 12.0
}

pub fn render_invoice_pdf(
    invoice: &InvoiceSummary,
    context: &InvoicePdfContext,
) -> Result<Vec<u8>, AppError> {
    let title = invoice_title(invoice);
    let (doc, page1, layer1) = PdfDocument::new(&title, Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), "Layer 1");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(redacted_internal_from)?;
    let font_bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(redacted_internal_from)?;
    let layer = doc.get_page(page1).get_layer(layer1);
    set_text_color_black(&layer);

    let mut y = draw_header(
        &layer,
        &font,
        &font_bold,
        context.business_name.trim(),
        &title,
        invoice.issue_date.as_deref(),
        invoice.due_date.as_deref(),
    );
    y = draw_customer_table(
        &layer,
        &font,
        &font_bold,
        y,
        &invoice.counterparty_name,
    );
    y = draw_line_items_table(&layer, &font, &font_bold, y, invoice);
    y = draw_totals_table(&layer, &font, &font_bold, y, invoice);

    for footer in compliance_footer_lines(context) {
        write_text_at(&layer, &font, BODY_FONT_PT, MARGIN_LEFT_MM, y, &footer);
        y += 6.0;
    }

    save_document(doc)
}

fn save_document(doc: PdfDocumentReference) -> Result<Vec<u8>, AppError> {
    let mut buffer = Vec::new();
    doc.save(&mut BufWriter::new(&mut buffer))
        .map_err(redacted_internal_from)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::{
        compliance_footer_lines, format_sek_minor, format_thousands_se, invoice_title,
        InvoicePdfContext,
    };
    use crate::invoicing::{InvoiceLine, InvoiceSummary};

    fn sample_invoice() -> InvoiceSummary {
        InvoiceSummary {
            id: "inv-1".to_string(),
            counterparty_id: "cp-1".to_string(),
            counterparty_name: "Kund AB".to_string(),
            status: "issued".to_string(),
            invoice_kind: "standard".to_string(),
            invoice_number: Some("2026-0001".to_string()),
            source_invoice_id: None,
            issue_date: Some("2026-07-12".to_string()),
            due_date: None,
            total_ex_vat_minor: 10_001_00,
            total_vat_minor: 0,
            total_inc_vat_minor: 10_001_00,
            pdf_job_id: None,
            pdf_document_id: None,
            voucher_id: None,
            payment_voucher_id: None,
            lines: vec![InvoiceLine {
                id: "line-1".to_string(),
                line_order: 1,
                description: "Konsulttjänster".to_string(),
                quantity: 1,
                unit_price_minor: 10_001_00,
                vat_rate_bp: 0,
                account_number: "3010".to_string(),
                line_ex_vat_minor: 10_001_00,
                line_vat_minor: 0,
            }],
        }
    }

    #[test]
    fn format_sek_minor_uses_swedish_grouping() {
        assert_eq!(format_sek_minor(10_001_00), "10\u{00a0}001,00 kr");
        assert_eq!(format_sek_minor(0), "0,00 kr");
    }

    #[test]
    fn format_thousands_se_groups_from_right() {
        assert_eq!(format_thousands_se(10001), "10\u{00a0}001");
    }

    #[test]
    fn invoice_title_uses_kreditfaktura_for_credit_notes() {
        let mut invoice = sample_invoice();
        invoice.invoice_kind = "credit_note".to_string();
        assert_eq!(invoice_title(&invoice), "KREDITFAKTURA 2026-0001");
    }

    #[test]
    fn compliance_footer_includes_f_skatt_and_vat_exemption() {
        let context = InvoicePdfContext {
            business_name: "Test AB".to_string(),
            owner_name: "Anna".to_string(),
            tax_status: "f_skatt".to_string(),
            vat_status: "exempt_low_turnover".to_string(),
        };
        let lines = compliance_footer_lines(&context);
        assert!(lines.iter().any(|line| line.contains("F-skatt")));
        assert!(lines.iter().any(|line| line.contains("Momsbefriad")));
    }

    #[test]
    fn render_invoice_pdf_produces_pdf_bytes() {
        let invoice = sample_invoice();
        let context = InvoicePdfContext {
            business_name: "Konsult AB".to_string(),
            owner_name: "Anna Svensson".to_string(),
            tax_status: "f_skatt".to_string(),
            vat_status: "exempt_low_turnover".to_string(),
        };
        let bytes = super::render_invoice_pdf(&invoice, &context).expect("render pdf");
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1_000);
    }
}
