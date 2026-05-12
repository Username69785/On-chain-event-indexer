WITH ranked_duplicates AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY
                tracked_owner,
                signature,
                source_owner,
                destination_owner,
                source_token_account,
                destination_token_account,
                token_mint,
                token_program,
                amount_raw,
                amount_ui,
                decimals,
                asset_type,
                transfer_type,
                direction,
                instruction_idx,
                inner_idx,
                authority,
                slot,
                block_time
            ORDER BY created_at, id
        ) AS row_number
    FROM public.token_transfers
)
DELETE FROM public.token_transfers
WHERE id IN (
    SELECT id
    FROM ranked_duplicates
    WHERE row_number > 1
);

CREATE UNIQUE INDEX idx_token_transfers_natural_unique
ON public.token_transfers (
    tracked_owner,
    signature,
    source_owner,
    destination_owner,
    source_token_account,
    destination_token_account,
    token_mint,
    token_program,
    amount_raw,
    amount_ui,
    decimals,
    asset_type,
    transfer_type,
    direction,
    instruction_idx,
    inner_idx,
    authority,
    slot,
    block_time
)
NULLS NOT DISTINCT;
