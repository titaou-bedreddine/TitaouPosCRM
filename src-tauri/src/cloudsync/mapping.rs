//! Pure field conversions between TitaouPOS (whole DZD, local ints) and
//! TitaouCRM (integer centimes, Postgres uuids). Every money crossing the
//! boundary goes through [dzd_to_centimes]/[centimes_to_dzd] — the single
//! place the ×100 rule lives.

/// POS works in whole DZD; CRM stores integer centimes. 1 DZD = 100 centimes.
pub const DZD_TO_CENTIMES: i64 = 100;

/// Push: whole DZD → centimes (1500 DZD → 150_000).
pub fn dzd_to_centimes(dzd: i64) -> i64 {
    dzd.checked_mul(DZD_TO_CENTIMES).unwrap_or(i64::MAX)
}

/// Pull: centimes → whole DZD, rounded to nearest (150_000 → 1500).
/// The CRM's minor-unit precision (a fraction of a centime doesn't exist in
/// cash practice) collapses safely.
pub fn centimes_to_dzd(centimes: i64) -> i64 {
    (centimes as f64 / DZD_TO_CENTIMES as f64).round() as i64
}

/// POS payment method → CRM payments.method enum.
/// cash → cash; cheque → cheque; tpe/versement/bank → transfer.
/// credit never produces a payment row (the remainder rides the balance).
pub fn crm_payment_method(pos_method: &str) -> Option<&'static str> {
    match pos_method {
        "cash" => Some("cash"),
        "cheque" => Some("cheque"),
        "tpe" | "versement" | "bank_transfer" | "transfer" => Some("transfer"),
        _ => None, // credit / unknown → no payment row
    }
}

/// CRM payments.method → POS display method.
pub fn pos_payment_method(crm_method: &str) -> &'static str {
    match crm_method {
        "cash" => "cash",
        "cheque" => "cheque",
        _ => "transfer",
    }
}

/// POS products carry trilingual names; the CRM has one `name` column.
/// French first (the org's working language), Arabic fallback, English last.
pub fn crm_product_name(name_fr: &str, name_ar: &str, name_en: &str) -> String {
    let fr = name_fr.trim();
    if !fr.is_empty() {
        return fr.to_string();
    }
    let ar = name_ar.trim();
    if !ar.is_empty() {
        return ar.to_string();
    }
    name_en.trim().to_string()
}

/// Fractional quantities pass through unchanged (both sides numeric), but
/// clamp the CRM's numeric(10,3) to 3 decimals when serializing.
pub fn qty_round(q: f64) -> f64 {
    (q * 1000.0).round() / 1000.0
}

/// Formats a numeric quantity for display: 2 → "2", 1.5 → "1.5", 0.75 → "0.75".
pub fn qty_display(q: f64) -> String {
    let r = qty_round(q);
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{}", r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_round_trip() {
        assert_eq!(dzd_to_centimes(1500), 150_000);
        assert_eq!(centimes_to_dzd(150_000), 1500);
        assert_eq!(centimes_to_dzd(dzd_to_centimes(27_500)), 27_500);
    }

    #[test]
    fn money_edges() {
        assert_eq!(dzd_to_centimes(0), 0);
        assert_eq!(centimes_to_dzd(0), 0);
        // Nearest-DZD rounding: 99 centimes (0.99 DZD) → 1 DZD — the CRM's
        // sub-DZD precision collapses to whole cash units on pull.
        assert_eq!(centimes_to_dzd(99), 1);
        assert_eq!(centimes_to_dzd(49), 0);
        // 150 centimes = 1.50 DZD; .round() halves away from zero → 2.
        assert_eq!(centimes_to_dzd(150), 2);
    }

    #[test]
    fn payment_method_map() {
        assert_eq!(crm_payment_method("cash"), Some("cash"));
        assert_eq!(crm_payment_method("tpe"), Some("transfer"));
        assert_eq!(crm_payment_method("versement"), Some("transfer"));
        assert_eq!(crm_payment_method("cheque"), Some("cheque"));
        assert_eq!(crm_payment_method("credit"), None);
        assert_eq!(crm_payment_method("anything"), None);
    }

    #[test]
    fn name_fallbacks() {
        assert_eq!(crm_product_name("Lait", "لبن", "Milk"), "Lait");
        assert_eq!(crm_product_name("", "لبن", "Milk"), "لبن");
        assert_eq!(crm_product_name("", "", "Milk"), "Milk");
        assert_eq!(crm_product_name("  ", "لبن", ""), "لبن");
    }

    #[test]
    fn quantities() {
        assert_eq!(qty_round(2.25), 2.25);
        assert_eq!(qty_round(0.12345), 0.123);
        assert_eq!(qty_display(2.0), "2");
        assert_eq!(qty_display(1.5), "1.5");
        assert_eq!(qty_display(0.75), "0.75");
    }
}
