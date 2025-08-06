use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::Semaphore;
use url::Url;

static URI_LOCK_MAP: Lazy<DashMap<Url, Arc<Semaphore>>> = Lazy::new(DashMap::new);

fn get_flag(url: &Url) -> Arc<Semaphore> {
    URI_LOCK_MAP
        .entry(url.clone())
        .or_insert_with(|| Arc::new(Semaphore::new(1)))
        .clone()
}

pub async fn uri_lock(url: &Url) {
    let sem = get_flag(url);
    sem.acquire().await.unwrap().forget();
}

pub fn uri_unlock(url: &Url) {
    let sem = get_flag(url);
    sem.add_permits(1);
}

pub async fn uri_wait(url: &Url) {
    let sem = get_flag(url);
    let permit = sem.acquire().await.unwrap();
    drop(permit);
}
