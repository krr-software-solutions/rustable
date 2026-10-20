ALTER TABLE billing.invoices ADD COLUMN due_date date;

-- Backfill: historical invoices were all net-30.
UPDATE billing.invoices
   SET due_date = (created_at AT TIME ZONE 'UTC')::date + 30
 WHERE due_date IS NULL;

-- Add the check without scanning the table under an ACCESS EXCLUSIVE lock,
-- then validate it separately (only takes SHARE UPDATE EXCLUSIVE).
ALTER TABLE billing.invoices
    ADD CONSTRAINT invoices_due_after_created
    CHECK (due_date >= (created_at AT TIME ZONE 'UTC')::date) NOT VALID;

ALTER TABLE billing.invoices VALIDATE CONSTRAINT invoices_due_after_created;
