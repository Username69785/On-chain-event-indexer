-- Change signatures PK from (signature) to (owner_address, signature)
-- so the same transaction can be tracked for multiple addresses.

ALTER TABLE public.signatures
    DROP CONSTRAINT signatures_pk;

ALTER TABLE public.signatures
    ADD CONSTRAINT signatures_pk PRIMARY KEY (owner_address, signature);
