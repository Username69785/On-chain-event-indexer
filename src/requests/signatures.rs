//! Module for fetching transaction signatures.
//!
//! Implements logic for querying the transaction history of a specific address,
//! including pagination and filtering by time.

// TODO:
// - get_signatures(address, ...) function delegating the HTTP request via http.rs
// - SignaturesPage handling logic
// - Time-based filtering: capability to stop pagination when signatures prior to a cutoff time are reached
