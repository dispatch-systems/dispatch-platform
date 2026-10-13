-- What Documents keeps for a DSP. Shipped migrations never change: add the next one instead.

-- Whether the connection reaches everything in the account's Drive, as Dispatch asks of every
-- connection since. One made before reaches only the files Dispatch made or was given, until
-- someone reconnects it.
ALTER TABLE documents_connection ADD COLUMN full_access INTEGER NOT NULL DEFAULT 0;
