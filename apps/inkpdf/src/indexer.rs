//! Reads the text of a document's pages on a thread of its own, behind the frame, nearest pages first, and hands the pages over as they
//! are done (the shell takes them each frame, as it takes finished tiles). It has its own parse of the file: the render pool's threads are
//! never held up by it. Dropping it stops the thread (when the file is replaced, a new one is made).

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use crate::pagetext::PageTextView;
use crate::pdf;

/// A page of the file, read: its text, or `None` when it could not be read.
pub type Read = (u32, Option<Arc<dyn PageTextView>>);

pub struct Indexer {
    wish: Sender<Vec<u32>>,
    done: Receiver<Read>,
    left: Arc<AtomicUsize>,
}

impl Indexer {
    /// Start reading the pages of `bytes` (nothing is read until pages are asked for).
    pub fn start(bytes: Arc<Vec<u8>>) -> Indexer {
        let (wish, wishes) = channel::<Vec<u32>>();
        let (sent, done) = channel::<Read>();
        let left = Arc::new(AtomicUsize::new(0));
        let remaining = left.clone();
        let spawned = std::thread::Builder::new().name("inkpdf-indexer".into()).spawn(move || {
            let mut source = pdf::native().text_source(bytes);
            let mut queue: VecDeque<u32> = VecDeque::new();
            let mut finished: HashSet<u32> = HashSet::new();
            loop {
                // With nothing to do, wait for a wish; with work, only look whether there is a newer one.
                let wished = if queue.is_empty() {
                    match wishes.recv() {
                        Ok(w) => Some(w),
                        Err(_) => return,
                    }
                } else {
                    match wishes.try_recv() {
                        Ok(w) => Some(w),
                        Err(TryRecvError::Empty) => None,
                        Err(TryRecvError::Disconnected) => return,
                    }
                };
                if let Some(wish) = wished {
                    queue = wish.into_iter().filter(|p| !finished.contains(p)).collect();
                    remaining.store(queue.len(), Ordering::Relaxed);
                }
                let Some(page) = queue.pop_front() else { continue };
                if finished.contains(&page) {
                    continue;
                }
                // (A page the engine panics on or refuses is given up, not tried again.)
                let text = source.page(page as usize).ok().map(Arc::<dyn PageTextView>::from);
                finished.insert(page);
                remaining.store(queue.len(), Ordering::Relaxed);
                if sent.send((page, text)).is_err() {
                    return;
                }
            }
        });
        if let Err(e) = spawned {
            eprintln!("could not start the text reader: {e}");
        }
        Indexer { wish, done, left }
    }

    /// Replace what is to be read with exactly these pages of the file, most urgent first.
    pub fn want(&self, pages: Vec<u32>) {
        self.left.store(pages.len(), Ordering::Relaxed);
        let _ = self.wish.send(pages);
    }

    /// A page that is read, if there is one.
    pub fn try_recv(&self) -> Option<Read> {
        self.done.try_recv().ok()
    }

    /// Whether pages are still to be read (the shell keeps drawing frames to take them).
    pub fn busy(&self) -> bool {
        self.left.load(Ordering::Relaxed) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{pdf_with_links, sample_pdf};
    use std::time::{Duration, Instant};

    fn collect(indexer: &Indexer, n: usize) -> Vec<Read> {
        let mut out = Vec::new();
        let started = Instant::now();
        while out.len() < n && started.elapsed() < Duration::from_secs(20) {
            match indexer.try_recv() {
                Some(r) => out.push(r),
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        out
    }

    #[test]
    fn it_reads_the_pages_asked_for_in_the_order_asked_and_each_once() {
        let indexer = Indexer::start(Arc::new(sample_pdf()));
        indexer.want(vec![0, 0, 0]);
        let got = collect(&indexer, 1);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, 0);
        assert_eq!(got[0].1.as_ref().map(|t| t.plain_text()), Some("Hello".to_string()));
        // Asking again for a page that was read does nothing more; a page that is not there is given up, with no text.
        indexer.want(vec![0, 7]);
        let got = collect(&indexer, 1);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].0, got[0].1.is_none()), (7, true));
        std::thread::sleep(Duration::from_millis(50));
        assert!(indexer.try_recv().is_none() && !indexer.busy());
    }

    #[test]
    fn a_newer_wish_replaces_the_older_and_a_file_that_is_not_a_pdf_gives_no_text_and_no_panic() {
        let indexer = Indexer::start(Arc::new(pdf_with_links()));
        indexer.want((0..50).collect());
        indexer.want(vec![0]);
        let got = collect(&indexer, 1);
        assert!(!got.is_empty());
        let broken = Indexer::start(Arc::new(b"not a pdf".to_vec()));
        broken.want(vec![0, 1]);
        let got = collect(&broken, 2);
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|(_, t)| t.is_none()));
    }

    #[test]
    fn dropping_it_stops_the_thread() {
        let indexer = Indexer::start(Arc::new(sample_pdf()));
        indexer.want(vec![0]);
        let _ = collect(&indexer, 1);
        drop(indexer);
    }
}
