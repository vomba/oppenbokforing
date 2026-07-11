ALTER TABLE invoices ADD COLUMN pdf_document_id TEXT REFERENCES documents(id);
