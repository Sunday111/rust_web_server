use std::thread::JoinHandle;

use crate::Result;
use crate::WebServerError;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

pub struct ThreadPool {
    _workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Worker {
    id: usize,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
        let thread = std::thread::spawn(move || loop {
            let maybe_job = receiver.lock().unwrap().recv();
            match maybe_job {
                Ok(job) => job(),
                Err(_) => break,
            };
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}

impl ThreadPool {
    pub fn new(threads_count: usize) -> Result<ThreadPool> {
        if threads_count == 0 {
            return Err(WebServerError(
                "Threads count could not be zero.".to_string(),
            ));
        }

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(threads_count);

        for id in 0..threads_count {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        drop(receiver);

        Ok(ThreadPool {
            _workers: workers,
            sender: Some(sender),
        })
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        self.sender.as_ref().expect("Sender was deleted").send(job).unwrap();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            println!("Dropping sender");
            drop(sender);
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        println!("Worker {} is closing", self.id);
        if let Some(handle) = self.thread.take() {
            if let Err(err) = handle.join() {
                println!("Worker finished with an error: {:#?}", err);
            }
        }
    }
}
