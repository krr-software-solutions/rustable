-- All readers moved to billing.customers in the August release.
-- Breaking for any app version older than 2025.08.
ALTER TABLE billing.invoices DROP COLUMN metadata;
