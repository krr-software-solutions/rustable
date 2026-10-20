-- Reverse order of creation.
DROP TRIGGER link_hits_bump_count ON link_hits;
DROP FUNCTION bump_link_hit_count();
DROP TABLE link_hits;
DROP TABLE links;
DROP TYPE link_visibility;
