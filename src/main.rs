use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn main() -> Result<(), String> {

    let mut args = std::env::args().skip(1);
    let mut file_arg: Option<String> = None;
    let mut workers: Option<usize> = None;
    let mut timeout_secs: Option<u64> = None;
    let mut retries: Option<u32> = None;
    let mut urls: Vec<String> = Vec::new();

    while let Some(arg) = args.next(){
        match arg.as_str(){
            "--file" => {
                let path = args.next().ok_or("Expecting file path after --file")?;
                file_arg = Some(path);
            }
            "--workers" => {
                let w_str = args.next().ok_or("Expected number after -workers")?;
                file_arg = Some(path);
            }
            "--timeout" => {
                let t_str = args.next().ok_or("Expected number after --timeout")?;
                timeout_secs = Some(t_str.parse().map_err(|e| e.to_string())?);
            }
            "--retries" => {
                let r_str = args.next().ok_or("Expected number after --retries")?;
                retries = Some(r_str.parse().map_err(|e| e.to_string())?);
            }
            other => {
                if other.starts_with('-'){
                    return Err(format!("Unknown arguement: {}", other));
                }
                urls.push(other);
            }
        }
    }

    if let Some(path) = file_arg{
        let f = File::open(&path)
            .map_err(|e| format!("Failed to open file {}: {}", path, e))?;
        for line in BufReader::new(f).lines(){
            let line = line.map_err(|e| format!("Error reading file {}: {}", path, e))?;
            let t = line.trim();
            if t.is_empty() || t.starts_with('#'){ continue; }
            urls.push(t.to_string());
        }
    }

    if urls.is_empty(){
        return Err("No URLS provided".into());
    }

    let num_workers = workers.unwrap_or_else(|| {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    });
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(5));
    let max_retries = retries.unwrap_or(0);

    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let client = Arc::new(client);

    let (task_tx, task_rx) = mpsc::channel::<String>();
    let (task_tx, task_rx) = mpsc::channel::<(String, Option<u16>, Option<String>, u128, u128)>();

    let task_rx = Arc::new(Mutex::new(task_rx));

    let mut handles = Vec::new();
    for _ in 0..num_workers{
        let task_rx = Arc::clone(&task_rx);
        let result_tx = result_tx.clone();
        let client = Arc::clone(&client);

        handles.push(thread::spawn(move || {
            while letOk(url) = task_rx.lock().unwrap().recv() {
                let start = Instant::now();
                let mut status_code = None;
                let mut error_msg = None;

                for attempt in 0..=max_retries {
                    match client.get(&url).send() {
                        Ok(resp) => {
                            status_code = Some(resp.status().as_u16());
                            break;
                        }
                        Err(e) if attempt < max_entries => {
                            thread::sleep(Duration::from_millis(100));
                            } Err(e) {
                                error_msg = Some(e.to_string());
                            }
                        }
                    }

                let elapsed_ms = start.elapsed().as_millis();
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as u128;

                let _ = result_tx.send((url.clone(), status_code, error_msg, elapsed_ms, timestamp));

            }
        }));
    }

    drop(result_tx);

    for url in urls {
        task_tx.send(url).map_err(|e| e.to_string())?;
    }
    drop(task_tx);

    let mut results: Vec::new();
    for (url, status, err, elapsed_ms, timestamp) in result_rx {
        if let Some(code) = status {
            println!("{} - status: {} - time: {}ms", url, code, elapsed_ms);
        } else if let Some(e) = &err {
            println!("{} - error: {} - time: {}ms", url, e, elapsed_ms);
        } 
        results.push((url, status, err, elapsed_ms, timestamp));
    }

    for h in handles {
        h.join().map_err(|_| "Thread panicked".to_string())?;
    }

    let mut out = File::create("status.json").map_err(|e| e.to_string())?;
    writeln!(out, "[").map_err(|e| e.to_string())?;
    for (i, (url, status, err, elapsed_ms, ts)) in results.iter().enumerate(){
        writeln!(out, " {{").map_err(|e| e.to_string())?;
        // let escaped_url = url.replace('\\', "\\\\").replace('\"', "\\\"");
        writeln!(outfile, " \"url\": \"{}\",", url).map_err(|e| e.to_string())?;
        if let Some(code) = status {
            writeln!(out, "   \"status_code\": "{}",", code).map_err(|e| e.to_string())?;
            writeln!(out, "   \"error\": null,").map_err(|e| e.to_string())?;
        } else {
            writeln!(out, "   \"status_code\": null,").map_err(|e| e.to_string())?;
            let err_field = err.as_ref().map(|e| format!("\"{}\"", e)).unwrap_or_else(|| "null".into());
            // if let Some(err) = err_opt {
                // let escaped_err = err.replace('\\', "\\\\").replace('\"', "\\\"");
                writeln!(out, "    \"error\": {},", err_field).map_err(|e| e.to_string())?;
            }
        writeln!(out, "    \"response_time_ms\": {},", elapsed_ms).map_err(|e| e.to_string())?;
        writeln!(out, "    \"timestamp\": {}", ts).map_err(|e| e.to_string())?;
        writeln!(out, "    }}{}", if i + 1 == results.len() {""} else {","}).map_err(|e| e.to_string())?;
        // if i + 1 ==results.len(){
            // writeln!(outfile, "  }}").map_err(|e| e.to_string())?;
        // } else {
            // writeln!(outfile, "  }},").map_err(|e| e.to_string())?;
        // }
    }
    writeln!(outfile, "]").map_err(|e| e.to_string())?;

    Ok(())
}
