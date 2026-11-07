-- How an operation's request body is encoded. JSON is the norm, but Swagger 2
-- formData and OpenAPI urlencoded bodies must not be sent as JSON.
ALTER TABLE openapi_operations ADD COLUMN body_format TEXT NOT NULL DEFAULT 'json';
