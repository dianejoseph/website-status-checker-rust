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
        let f = File::open(&path).map_err(|e| format!("Failed to open file {}: {}", path, e))?;
        for line in BufReader::new(file).lines(){
            let line = line.map_err(|e| format!("Error reading file {}: {}", path, e))?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#'){
                continue;
            }
            urls.push(trimmed.to_string());
        }
    }

    if urls.is_empty(){
        return Err("No URLS provided.".into());
    }

    let num_workers = workers.unwrap_or_else(|| {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    });
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(5));
    let max_retries = retries.unwrap_or(0);

    let client = reqwest::blocking::Client::builder().timeout(timeout).build().map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let client = Arc::new(client);

    let (task_tx, task_rx) = mpsc::channel::<(String, Option<u16>, Option<String>, u128, u128)>();
    let task_rx = Arc::new(Mutex::new(task_rx));

    let mut handles = Vec::new();
    for _ in 0..num_workers{
        let task_rx = Arc::clone(&task_rx);
        let result_tx = result_tx.clone();
        let client = Arc::clone(&client);

        let handle = thread::spawn(move || {

            while letOk(url) = task.lock().unwrap().recv(){
                let start = Instant::now();
                let mut status_code = None;
                let mut error_msg = None;

                for attempt in 0..=max_retries {
                    match client.get(&url).send() {
                        Ok(resp) => {
                            status_code = Some(resp.status().as_u16());
                            break;
                        }
                        Err(e) => {
                            if attempt == max_retries {
                                error_msg = Some(e.to_string());
                            } else {
                                thread::sleep(Duration::from_millis(100));
                            }
                        }
                    }
                }

                let elapsed_ms = start.elapsed().as_millis();
                let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::from_secs(0)).as_secs();

                let _ = result_tx.send((url.clone(), status_code, error_msg, elapsed_ms, timestamp));

            }
        });
        handles.push(handle);
    }

    drop(result_tx);

    for urls in urls {
        task_tx.send(url).map_err(|e| format!("Failed to send task: {}", e))?;
    }
    drop(task_tx);

    let mut results: Vec<(String, Option<u16>, Option<String>, u128, u128)> = Vec::new();
    for (url, status_opt, err_opt, elapsed_ms, timestamp) in result_rx {
        if let Some(code) = status_opt {
            println!("{} - status: {} - time: {}ms", url, code, elapsed_ms);
        } else if let Some(err) = &err_opt {
            println!("{} - status: {} - time: {}ms", url, err, elapsed_ms);
        } else {
            println!("{} - unknown result - time: {}ms", url, elapsed_ms);
        }
        result.push((url, status_opt, err_opt, elapsed, timestamp));
    }

    for handle in handles {
        handle.join().map_err(|_| "Thread panicked".to_string())?;
    }

    let mut outfile = File::create("status.json").map_err(|e| format!("Failed to create status.json: {}", e))?;
    writeln!(outfile, "[").map_err(|e| e.to_string())?;
    for (i, (url, status_opt, err_opt, elapsed_ms, timestamp)) in results.iter().enumerate(){
        writeln!(outfile, " {{").map_err(|e| e.to_string())?;
        let escaped_url = url.replace('\\', "\\\\").replace('\"', "\\\"");
        writeln!(outfile, " \"url\": \"{}\",", escaped_url).map_err(|e| e.to_string())?;
        if let Some(code) = status_opt {
            writeln!(outfile, "   \"status_code\": \"{}\",", code).map_err(|e| e.to_string())?;
            writeln!(outfile, "   \"error\": null,").map_err(|e| e.to_string())?;
        } else {
            writeln!(outfile, "   \"status_code\": null,").map_err(|e| e.to_string())?;
            if let Some(err) = err_opt {
                let escaped_err = err.replace('\\', "\\\\").replace('\"', "\\\"");
                writeln!(outfile, "    \"error\": null,").map_err(|e| e.to_string())?;
            }
        }
        writeln!(outfile, "    \"response_time_ms\": {},", elapsed_ms).map_err(|e| e.to_string())?;
        writeln!(outfile, "    \"timestamp\": {}", timestamp).map_err(|e| e.to_string())?;
        if i + 1 ==results.len(){
            writeln!(outfile, "  }}").map_err(|e| e.to_string())?;
        } else {
            writeln!(outfile, "  }},").map_err(|e| e.to_string())?;
        }
    }
    writeln!(outfile, "]").map_err(|e| e.to_string())?;

    Ok(())
}
