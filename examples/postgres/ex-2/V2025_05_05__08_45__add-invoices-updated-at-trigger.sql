ALTER TABLE billing.invoices
    ADD COLUMN updated_at timestamptz NOT NULL DEFAULT now();

CREATE OR REPLACE FUNCTION billing.touch_updated_at()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    -- Only bump the timestamp when something actually changed.
    IF NEW IS DISTINCT FROM OLD THEN
        NEW.updated_at := now();
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER invoices_touch_updated_at
    BEFORE UPDATE ON billing.invoices
    FOR EACH ROW
    EXECUTE FUNCTION billing.touch_updated_at();
