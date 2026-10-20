-- SHOP-118 rollback
-- TODO: should we put idx_orders_status back? nobody used it anyway ¯\_(ツ)_/¯
ALTER TABLE `orders`
  DROP INDEX `idx_orders_customer_created`,
  DROP INDEX `idx_orders_status_created`,
  ALGORITHM=INPLACE, LOCK=NONE;
