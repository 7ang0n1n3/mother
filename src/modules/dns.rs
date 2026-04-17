use anyhow::Result;
use hickory_resolver::TokioAsyncResolver;
use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::proto::rr::RecordType;
use tokio::sync::mpsc;

use super::OutputLine;

pub async fn run(target: String, tx: mpsc::Sender<OutputLine>) -> Result<()> {
    let target = target.trim().to_string();
    if target.is_empty() {
        tx.send(OutputLine::Error("NO TARGET SPECIFIED.".into())).await.ok();
        tx.send(OutputLine::Done).await.ok();
        return Ok(());
    }

    let (domain, qtype_str) = match target.split_once(' ') {
        Some((d, t)) => (d.trim().to_string(), t.trim().to_uppercase()),
        None => (target.clone(), "A".to_string()),
    };

    tx.send(OutputLine::Bright(format!(
        "DNS QUERY: {}  [TYPE {}]",
        domain.to_uppercase(),
        qtype_str
    )))
    .await
    .ok();

    let record_type = match qtype_str.as_str() {
        "A"     => RecordType::A,
        "AAAA"  => RecordType::AAAA,
        "MX"    => RecordType::MX,
        "NS"    => RecordType::NS,
        "TXT"   => RecordType::TXT,
        "CNAME" => RecordType::CNAME,
        "SOA"   => RecordType::SOA,
        "PTR"   => RecordType::PTR,
        "SRV"   => RecordType::SRV,
        "CAA"   => RecordType::CAA,
        other   => {
            tx.send(OutputLine::Error(format!(
                "UNSUPPORTED RECORD TYPE: {}. USE: A AAAA MX NS TXT CNAME SOA PTR SRV CAA",
                other
            )))
            .await
            .ok();
            tx.send(OutputLine::Done).await.ok();
            return Ok(());
        }
    };

    let resolver =
        TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

    match resolver.lookup(&domain as &str, record_type).await {
        Ok(lookup) => {
            let records: Vec<_> = lookup.records().iter().collect();
            if records.is_empty() {
                tx.send(OutputLine::Dim("  NO RECORDS FOUND.".into())).await.ok();
            } else {
                for record in records {
                    let data = record
                        .data()
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "<NO DATA>".into());
                    let line = format!(
                        "  {}\t{}\tIN\t{}\t{}",
                        record.name(),
                        record.ttl(),
                        record.record_type(),
                        data
                    )
                    .to_uppercase();
                    tx.send(OutputLine::Normal(line)).await.ok();
                }
            }
        }
        Err(e) => {
            tx.send(OutputLine::Error(format!(
                "QUERY FAILED: {}",
                e.to_string().to_uppercase()
            )))
            .await
            .ok();
        }
    }

    tx.send(OutputLine::Dim("QUERY COMPLETE.".into())).await.ok();
    tx.send(OutputLine::Done).await.ok();
    Ok(())
}
