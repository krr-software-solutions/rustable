DROP POLICY invoices_tenant_isolation ON billing.invoices;
ALTER TABLE billing.invoices DISABLE ROW LEVEL SECURITY;
REVOKE SELECT, INSERT, UPDATE ON billing.invoices FROM ledgerly_app;
REVOKE USAGE ON SCHEMA billing FROM ledgerly_app;
-- The role is cluster-wide and may be shared; deliberately left in place.
