ALTER TABLE billing.invoices DROP CONSTRAINT invoices_due_after_created;
ALTER TABLE billing.invoices DROP COLUMN due_date;
