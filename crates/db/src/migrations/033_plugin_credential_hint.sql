-- Recommended credential header derived from the document's security scheme.
-- Without this the picker always defaults to `Authorization`, which is wrong
-- for apps that expect `X-API-Key` or a vendor-specific header.
ALTER TABLE plugins ADD COLUMN credential_header_hint TEXT;
