pub fn mask_addr(addr: &str) -> String {
    const KEEP: usize = 5;
    if addr.len() <= KEEP * 2 + 3 {
        return addr.to_string();
    }
    let start = &addr[..KEEP];
    let end = &addr[addr.len() - KEEP..];
    format!("{start}...{end}")
}
