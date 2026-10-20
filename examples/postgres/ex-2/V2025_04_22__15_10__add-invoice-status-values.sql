-- Enum values can be added but never removed: this migration is one-way.
-- (Before PG12 ADD VALUE could not run inside a transaction block at all.)
ALTER TYPE billing.invoice_status ADD VALUE IF NOT EXISTS 'void';
ALTER TYPE billing.invoice_status ADD VALUE IF NOT EXISTS 'uncollectible' AFTER 'paid';
