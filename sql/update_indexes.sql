BEGIN;

-- If this unique index from previous attempts still exists, recreate it
-- with the same final definition inside one script.
DROP INDEX IF EXISTS public.idx_processing_data_address;

-- Primary key constraints and their backing indexes.
ALTER TABLE public.processing_data
    ADD CONSTRAINT processing_data_pkey PRIMARY KEY (id);

ALTER TABLE public.signatures
    ADD CONSTRAINT signatures_pk PRIMARY KEY (signature);

ALTER TABLE public.token_transfers
    ADD CONSTRAINT token_transfers_pkey PRIMARY KEY (id);

ALTER TABLE public.transactions
    ADD CONSTRAINT transactions_pkey PRIMARY KEY (owner_address, signature);

-- Regular indexes for current query patterns.
CREATE UNIQUE INDEX idx_processing_data_address
    ON public.processing_data USING btree (address);

CREATE INDEX idx_processing_data_status_created_at
    ON public.processing_data USING btree (status, created_at);

CREATE INDEX idx_signatures_unprocessed_owner_time
    ON public.signatures USING btree (owner_address, block_time DESC)
    WHERE is_processed = FALSE;

COMMIT;
