CREATE TABLE public.processing_data (
    id bigint NOT NULL,
    address text NOT NULL,
    status text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    worker_id smallint,
    tx_limit smallint NOT NULL,
    requested_hours smallint NOT NULL
);

CREATE SEQUENCE public.processing_data_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.processing_data_id_seq OWNED BY public.processing_data.id;

CREATE TABLE public.signatures (
    block_time bigint,
    owner_address text NOT NULL,
    signature text NOT NULL,
    is_processed boolean DEFAULT false
);

CREATE TABLE public.token_transfers (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    tracked_owner text CONSTRAINT token_transfers_owner_address_not_null NOT NULL,
    signature text NOT NULL,
    token_mint text,
    decimals integer,
    slot bigint NOT NULL,
    block_time bigint,
    created_at timestamp with time zone DEFAULT now(),
    token_program text,
    source_owner text,
    destination_owner text,
    source_token_account text,
    destination_token_account text,
    amount_raw numeric(40,0) NOT NULL,
    amount_ui numeric,
    transfer_type text NOT NULL,
    asset_type text NOT NULL,
    direction text DEFAULT 'unknown'::text NOT NULL,
    authority text,
    instruction_idx integer,
    inner_idx integer
);

CREATE TABLE public.transactions (
    owner_address text NOT NULL,
    signature text NOT NULL,
    slot bigint NOT NULL,
    block_time bigint NOT NULL,
    fee bigint NOT NULL,
    compute_units integer,
    err jsonb,
    num_signers integer,
    num_instructions integer
);

ALTER TABLE ONLY public.processing_data ALTER COLUMN id SET DEFAULT nextval('public.processing_data_id_seq'::regclass);

ALTER TABLE ONLY public.processing_data
    ADD CONSTRAINT processing_data_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.signatures
    ADD CONSTRAINT signatures_pk PRIMARY KEY (signature);

ALTER TABLE ONLY public.token_transfers
    ADD CONSTRAINT token_transfers_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_pkey PRIMARY KEY (owner_address, signature);

CREATE UNIQUE INDEX idx_processing_data_address ON public.processing_data USING btree (address);

CREATE INDEX idx_processing_data_status_created_at ON public.processing_data USING btree (status, created_at);

CREATE INDEX idx_signatures_unprocessed_owner_time ON public.signatures USING btree (owner_address, block_time DESC) WHERE (is_processed = false);
