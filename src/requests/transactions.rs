//! Module for fetching transactions.
//!
//! Orchestrates the retrieval of detailed transaction information:
//! chunking requests, concurrent execution, and retry logic.

// TODO:
// - get_transaction(...) or get_transactions_batch(...) function
// - Orchestration: chunk/fetch logic (splitting signature list into small chunks)
// - Retry orchestration logic when facing errors (e.g., rate limits)
