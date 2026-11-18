use codepact::{AnalysisCancelled, AnalysisSession, SourceFile};

fn main() -> Result<(), AnalysisCancelled> {
    let mut session = AnalysisSession::default();
    let files = [SourceFile::new("store.py", "store", "class Store: pass\n")];
    let token = session.cancellation_token();

    // Cancel before starting the query so the demonstration has no timing race.
    token.cancel();
    match session.inspect_cancellable(&files, "3.13") {
        Err(AnalysisCancelled) => println!("Cancelled. No result was published."),
        Ok(_) => panic!("expected cancellation"),
    }

    let latest = [SourceFile::new("store.py", "store", "class Cache: pass\n")];
    let result = session.inspect_cancellable(&latest, "3.13")?;
    println!(
        "Completed revision {} with coverage {}",
        result.revision, result.inspection.coverage.status
    );
    Ok(())
}
