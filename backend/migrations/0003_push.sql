-- iOS push (phase 6): one APNs device token per paired device.
-- `push_env` is `sandbox` (Xcode debug builds) or `production` (TestFlight, ad hoc, App Store).
ALTER TABLE devices ADD COLUMN push_token TEXT;
ALTER TABLE devices ADD COLUMN push_env TEXT CHECK (push_env IN ('sandbox', 'production'));
ALTER TABLE devices ADD COLUMN push_updated_at INTEGER;
CREATE UNIQUE INDEX devices_push_token ON devices (push_token) WHERE push_token IS NOT NULL;
