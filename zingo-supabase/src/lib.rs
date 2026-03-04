//! Sync Zcash wallet transactions to Supabase.
//!
//! This crate provides a [`SupabaseClient`] that accepts zingolib
//! [`ValueTransfer`]s and upserts them into a Supabase table.

#![warn(missing_docs)]

use serde::Serialize;
use zingolib::wallet::summary::data::ValueTransfer;

/// Errors that can occur when interacting with Supabase.
#[derive(Debug, thiserror::Error)]
pub enum SupabaseError {
    /// An HTTP request failed.
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    /// The Supabase API returned a non-success status code.
    #[error("Supabase upsert failed ({status}): {body}")]
    Upsert {
        /// The HTTP status code.
        status: reqwest::StatusCode,
        /// The response body.
        body: String,
    },
}

/// A flattened row suitable for upserting into a Supabase table.
#[derive(Debug, Serialize)]
pub struct ValueTransferRow {
    /// Composite key: `{txid}|{kind}|{recipient_address}|{pool_received}|{value}`.
    pub id: String,
    /// Transaction ID.
    pub txid: String,
    /// Unix timestamp.
    pub datetime: u32,
    /// Confirmation status.
    pub status: String,
    /// Block height of the transaction.
    pub blockheight: u64,
    /// Transaction fee in zatoshis, if known.
    pub transaction_fee: Option<u64>,
    /// ZEC price at the time of the transaction, if known.
    pub zec_price: Option<f32>,
    /// Transfer kind (e.g. "sent", "received").
    pub kind: String,
    /// Value in zatoshis.
    pub value: u64,
    /// Recipient address, if applicable.
    pub recipient_address: Option<String>,
    /// Shielded pool that received the funds, if applicable.
    pub pool_received: Option<String>,
    /// Memos attached to the transaction.
    pub memos: Vec<String>,
}

impl From<&ValueTransfer> for ValueTransferRow {
    fn from(vt: &ValueTransfer) -> Self {
        let txid = vt.txid.to_string();
        let kind = vt.kind.to_string();
        let recipient_address = vt.recipient_address.clone();
        let pool = vt.pool_received.as_deref().unwrap_or_default();
        let id = format!(
            "{txid}|{kind}|{}|{pool}|{}",
            recipient_address.as_deref().unwrap_or_default(),
            vt.value
        );

        Self {
            id,
            txid,
            datetime: vt.datetime,
            status: vt.status.to_string(),
            blockheight: u32::from(vt.blockheight) as u64,
            transaction_fee: vt.transaction_fee,
            zec_price: vt.zec_price,
            kind,
            value: vt.value,
            recipient_address,
            pool_received: vt.pool_received.clone(),
            memos: vt.memos.clone(),
        }
    }
}

/// Client for upserting data into a Supabase table.
pub struct SupabaseClient {
    client: reqwest::Client,
    url: String,
    api_key: String,
    table: String,
}

impl SupabaseClient {
    /// Create a new client.
    ///
    /// - `supabase_url`: the project URL (e.g. `https://xyz.supabase.co`)
    /// - `api_key`: the `anon` or `service_role` key
    /// - `table`: the target table name
    pub fn new(supabase_url: &str, api_key: &str, table: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: supabase_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            table: table.to_string(),
        }
    }

    /// Upsert rows into the configured table using `resolution=merge-duplicates`.
    pub async fn upsert_rows(&self, rows: &[ValueTransferRow]) -> Result<(), SupabaseError> {
        if rows.is_empty() {
            return Ok(());
        }

        let url = format!("{}/rest/v1/{}", self.url, self.table);

        let resp = self
            .client
            .post(&url)
            .header("apikey", &self.api_key)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("Prefer", "resolution=merge-duplicates")
            .json(rows)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SupabaseError::Upsert { status, body });
        }

        Ok(())
    }
}

/// Convert a slice of [`ValueTransfer`]s and upsert them in one call.
pub async fn upsert_value_transfers(
    client: &SupabaseClient,
    transfers: &[ValueTransfer],
) -> Result<(), SupabaseError> {
    let rows: Vec<ValueTransferRow> = transfers.iter().map(ValueTransferRow::from).collect();
    client.upsert_rows(&rows).await
}
