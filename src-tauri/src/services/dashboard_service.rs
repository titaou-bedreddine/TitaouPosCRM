use crate::database::DbState;
use crate::models::{DashboardStats, TopProductStat};
use rusqlite::Result;

pub fn get_stats(db: &DbState, start_date: Option<String>, end_date: Option<String>) -> Result<DashboardStats, String> {
    let conn = db.conn.lock().unwrap();

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    // The "All" quick filter sends empty strings, not null — treat those as
    // "no bound" instead of comparing dates against '' (which matched
    // nothing and made the All filter show zeros).
    let s_date = start_date.filter(|d| !d.trim().is_empty()).unwrap_or(today.clone());
    let e_date = end_date.filter(|d| !d.trim().is_empty()).unwrap_or(today);

    let today_sales: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(total_amount), 0) FROM sales WHERE DATE(created_at) >= ?1 AND DATE(created_at) <= ?2 AND status = 'completed'",
            rusqlite::params![s_date, e_date],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let today_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sales WHERE DATE(created_at) >= ?1 AND DATE(created_at) <= ?2 AND status = 'completed'",
            rusqlite::params![s_date, e_date],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let returns_amount: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(total_price), 0) FROM sale_items si JOIN sales s ON si.sale_id = s.id WHERE si.is_refunded = 1 AND DATE(s.created_at) >= ?1 AND DATE(s.created_at) <= ?2",
            rusqlite::params![s_date, e_date],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let low_stock_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM products WHERE current_stock <= min_stock AND current_stock > 0 AND is_active = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let out_of_stock_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM products WHERE current_stock <= 0 AND is_active = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let today_expenses: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM expenses WHERE date >= ?1 AND date <= ?2",
            rusqlite::params![s_date, e_date],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let active_cash_expected: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(expected_cash), 0) FROM cash_sessions WHERE status = 'open'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let net_revenue = today_sales - returns_amount;
    let cost_of_goods = (net_revenue as f64 * 0.7) as i64; // Approximation based on cost
    let gross_profit = net_revenue - cost_of_goods;
    let today_profit = gross_profit - today_expenses;
    let average_basket = if today_count > 0 { today_sales / today_count } else { 0 };

    // LAN: today's sales split per terminal (PC name).
    let sales_by_terminal: Vec<crate::models::TerminalSalesStat> = {
        let mut stmt = conn
            .prepare(
                "SELECT COALESCE(NULLIF(terminal_name, ''), 'Unknown PC') as term,
                        COALESCE(SUM(total_amount), 0), COUNT(*)
                 FROM sales
                 WHERE date(created_at) = date('now','localtime') AND status = 'completed'
                 GROUP BY term ORDER BY 2 DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(crate::models::TerminalSalesStat {
                    terminal: row.get(0)?,
                    total: row.get(1)?,
                    count: row.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Fetch top products
    let mut top_stmt = conn.prepare(
        "SELECT p.name_ar, COALESCE(c.name_ar, 'General'), SUM(si.quantity), SUM(si.total_price)
         FROM sale_items si
         JOIN products p ON si.product_id = p.id
         LEFT JOIN categories c ON p.category_id = c.id
         GROUP BY si.product_id
         ORDER BY SUM(si.total_price) DESC LIMIT 10",
    ).map_err(|e| e.to_string())?;

    let top_rows = top_stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        let cat: String = row.get(1)?;
        let qty: f64 = row.get(2)?;
        let rev: i64 = row.get(3)?;
        let cost = (rev as f64 * 0.7) as i64;
        let profit = rev - cost;
        Ok(TopProductStat {
            product_name: name,
            category_name: cat,
            sold_qty: qty,
            revenue: rev,
            cost,
            profit,
        })
    }).map_err(|e| e.to_string())?;

    let top_products: Vec<TopProductStat> = top_rows.filter_map(|r| r.ok()).collect();

    Ok(DashboardStats {
        today_sales,
        today_transactions_count: today_count,
        today_profit,
        low_stock_count,
        out_of_stock_count,
        active_cash_expected,
        today_expenses,
        returns_amount,
        net_revenue,
        cost_of_goods,
        gross_profit,
        average_basket,
        top_products,
        sales_by_terminal,
    })
}