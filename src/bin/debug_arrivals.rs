use board::{fetch_board, format_clock, format_time, recalculate_and_filter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Fetching Redwood City Caltrain (stops 70141 northbound, 70142 southbound)\n");

    let (mut arrivals, clock_offset_ms) = fetch_board().await?;

    arrivals.sort_by_key(|arrival| arrival.departure_time_ms);
    println!("=== Live feed ===");
    for arrival in &arrivals {
        println!(
            "  {:1}  #{:<4} {:<10} {:<18} arr {:<5} dep {:<5}  {}",
            arrival.route_label,
            arrival.trip_id,
            arrival.service,
            arrival.destination,
            format_clock(arrival.arrival_time_ms),
            format_clock(arrival.departure_time_ms),
            format_time(arrival.minutes)
        );
    }

    recalculate_and_filter(&mut arrivals, clock_offset_ms);

    println!("\n=== On the board ===");
    for arrival in &arrivals {
        println!(
            "  {:1}  #{:<4} {:<10} {:<18} arr {:<5} dep {:<5}  {}",
            arrival.route_label,
            arrival.trip_id,
            arrival.service,
            arrival.destination,
            format_clock(arrival.arrival_time_ms),
            format_clock(arrival.departure_time_ms),
            format_time(arrival.minutes)
        );
    }

    Ok(())
}
