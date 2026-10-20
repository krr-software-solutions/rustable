-- SHOP-118: "my orders" page does customer_id = ? ORDER BY created_at DESC.
-- Replace the single-column status index with a composite one; status
-- alone is low-cardinality and the optimizer never picks it (checked EXPLAIN on prod replica).

ALTER TABLE `orders`
  ADD INDEX `idx_orders_customer_created` (`customer_id`, `created_at` DESC),
  ALGORITHM=INPLACE, LOCK=NONE;

ALTER TABLE `orders`
  DROP INDEX `idx_orders_status`,
  ADD INDEX `idx_orders_status_created` (`status`, `created_at`),
  ALGORITHM=INPLACE, LOCK=NONE;
