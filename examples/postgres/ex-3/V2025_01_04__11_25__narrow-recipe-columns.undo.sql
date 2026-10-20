ALTER TABLE recipes DROP CONSTRAINT recipes_servings_reasonable;

ALTER TABLE recipes
    ALTER COLUMN name     SET DATA TYPE varchar(255),
    ALTER COLUMN servings SET DATA TYPE integer;
