DROP INDEX IF EXISTS idx_processing_data_address;
CREATE INDEX idx_processing_data_address ON public.processing_data USING btree (address);
ALTER TABLE signatures
ADD COLUMN is_processing boolean DEFAULT false;
ALTER TABLE signatures
ADD COLUMN processing_started_at timestamp with time zone;