-- The copy of each text filed and how far it had read the phone side's changes, which nothing reads
-- or writes since migration 9 filed each text on its own row (`phones_texts`). Their indexes go
-- with them. No release ever had them.
DROP TABLE IF EXISTS communications_texts;
DROP TABLE IF EXISTS communications_texting;
