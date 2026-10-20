DROP TRIGGER invoices_touch_updated_at ON billing.invoices;
DROP FUNCTION billing.touch_updated_at();
ALTER TABLE billing.invoices DROP COLUMN updated_at;
