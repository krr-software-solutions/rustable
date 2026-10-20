-- SHOP-133 rollback: audit rows are lost. Export them first if compliance asks.
DROP TRIGGER IF EXISTS `trg_orders_status_audit`;
DROP TABLE IF EXISTS `order_status_audit`;
