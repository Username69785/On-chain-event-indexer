//! Module responsible for executing HTTP requests.
//!
//! Handles integration with `reqwest`, semaphores to limit concurrent requests,
//! rate limiters, parsing of the `Retry-After` header, and logging (e.g., `body_snippet` for debugging).

// TODO:
// - Configure reqwest::Client
// - Add Semaphore for concurrency control
// - Add Rate Limiter logic
// - Implement Retry-After header parsing
// - Write a body_snippet function/macro to print a snippet of the response body on errors
