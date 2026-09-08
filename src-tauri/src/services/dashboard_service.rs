use crate::database::DbState;
use crate::models::{DashboardStats, TopProductStat};
use rusqlite::Result;

pub fn get_stats(db: &DbState, start_date: Option<String>, end_date: Option<String>) -> Result<DashboardStats, String> {
    let conn = db.conn.lock().unwrap();

    // The "All" quick filter sends empty strings, not null — those mean
    // NO BOUND (true all-time). A missing bound drops the SQL predicate
    // entirely; it must NOT silently narrow to "today" (field-found: the
    // All filter was showing only today's numbers).
    let s_date: Option<String> = start_date.filter(|d| !d.trim().is_empty());
    let e_date: Option<String> = end_date.filter(|d| !d.trim().is_empty());
    // Build the WHERE fragment once per table's date expression.
    let sales_where = match (&s_date, &e_date) {
        (Some(a), Some(b)) => format!("DATE(created_at) >= '{}' AND DATE(created_at) <= '{}' AND", a, b),
        (Some(a), None) => format!("DATE(created_at) >= '{}' AND", a),
        (None, Some(b)) => format!("DATE(created_at) <= '{}' AND", b),
        (None, None) => String::new(),
    };
    let sale_join_where = {
        let w = sales_where.replace("DATE(created_at)", "DATE(s.created_at)");
        w
    };
    let expenses_where = match (&s_date, &e_date) {
        (Some(a), Some(b)) => format!("date >= '{}' AND date <= '{}' AND", a, b),
        (Some(a), None) => format!("date >= '{}' AND", a),
        (None, Some(b)) => format!("date <= '{}' AND", b),
        (None, None) => String::new(),
    };

    let today_sales: i64 = conn
        .query_row(
            &format!("SELECT COALESCE(SUM(total_amount), 0) FROM sales WHERE {} status = 'completed'", sales_where),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let today_count: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM sales WHERE {} status = 'completed'", sales_where),
            rusqlite::params![],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);

    let returns_amount: i64 = conn
        .query_row(
            &format!("SELECT COALESCE(SUM(total_price), 0) FROM sale_items si JOIN sales s ON si.sale_id = s.id WHERE {} si.is_refunded = 1", sale_join_where),
            rusqlite::params![],
            |r| r.get::<_, i64>(0),
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
            &format!("SELECT COALESCE(SUM(amount), 0) FROM expenses WHERE {} 1=1", expenses_where),
            rusqlite::params![],
            |r| r.get::<_, i64>(0),
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
                &format!("SELECT COALESCE(NULLIF(terminal_name, ''), 'Unknown PC') as term,
                        COALESCE(SUM(total_amount), 0), COUNT(*)
                 FROM sales
                 WHERE {} status = 'completed'
                 GROUP BY term ORDER BY 2 DESC", sales_where),
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

    // Newly ADDED stock in the period: purchases + positive adjustments.
    // qty = units in, cost = what was paid, possible profit = current
    // retail value minus that cost (what the shop could earn if it sells).
    let new_stock: crate::models::NewStockStat = conn
        .query_row(
            &format!(
                "SELECT COALESCE(SUM(im.quantity), 0),
                        COALESCE(SUM(im.quantity * COALESCE(im.cost_at_time, 0)), 0),
                        COALESCE(SUM(im.quantity * COALESCE(p.sale_price, 0)), 0),
                        COUNT(DISTINCT im.product_id)
                 FROM inventory_movements im
                 LEFT JOIN products p ON im.product_id = p.id
                 WHERE {} im.type IN ('purchase', 'adjustment_inc')",
                sales_where.replace("DATE(created_at)", "DATE(im.created_at)")
            ),
            rusqlite::params![],
            |r| {
                let cost: i64 = r.get(1)?;
                let sale: i64 = r.get(2)?;
                Ok(crate::models::NewStockStat {
                    qty_added: r.get(0)?,
                    cost_total: cost,
                    sale_value: sale,
                    possible_profit: sale - cost,
                    product_count: r.get(3)?,
                })
            },
        )
        .unwrap_or_default();

    // Fetch top products
    let mut top_stmt = conn.prepare(
        &format!("SELECT p.name_ar, COALESCE(c.name_ar, 'General'), SUM(si.quantity), SUM(si.total_price)
         FROM sale_items si
         JOIN sales s ON si.sale_id = s.id
         JOIN products p ON si.product_id = p.id
         LEFT JOIN categories c ON p.category_id = c.id
         WHERE {} s.status = 'completed'
         GROUP BY si.product_id
         ORDER BY SUM(si.total_price) DESC LIMIT 10", sale_join_where),
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
        new_stock,
    })
}