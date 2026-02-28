use std::sync::Arc;
use tokio::sync::OnceCell;

#[derive(Clone)]
pub struct ElasticsearchReceiver {
    url: String,
    version: Arc<OnceCell<String>>,
}
