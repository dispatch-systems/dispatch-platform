-- Who marked a writeup signed, by their account, beside the name it was signed by: the person
-- the directory knows them as, and their photo, follow from it. Writeups signed before keep none.
ALTER TABLE conduct_writeups ADD COLUMN signed_by_id TEXT;
