mod bus;
mod marketdata;
mod strategy;
mod execution;
mod altdata;
mod core;

use anyhow::Result;
use bus::types::Bus;
use execution::actor::ExecutionActor;
use altdata::actor::AltDataActor;
use marketdata::actor::MarketDataActor;
use strategy::actor::StrategyActor;

use tracing::{error, info, info_span, Instrument};
use tokio_util::sync::CancellationToken;

use crate::core::types::Actor;

#[tokio::main]
async fn main()  -> Result<()>   {
    tracing_subscriber::fmt::init();
    // Root span for the supervisor/main thread
    let span = info_span!(
        "Supervisor",
        pid = %std::process::id(),
        version = env!("CARGO_PKG_VERSION"),
    );

    // logs below are inside "Supervisor"
    let _enter = span.enter();

    info!("Starting up");

    info!("Initializing shared pub/sub Bus");
    let bus = Bus::new();
    let shutdown = CancellationToken::new();

    info!("Building actors");
    let alt_data = AltDataActor::new(bus.clone(), shutdown.clone());
    let market_data = MarketDataActor::new(bus.clone(), shutdown.clone());
    let strat = StrategyActor::new(bus.clone(), shutdown.clone());
    let exec = ExecutionActor::new(bus.clone(), shutdown.clone());

    info!("Spawning actors");
    let mut actors = tokio::task::JoinSet::new();
    actors.spawn(alt_data.run().instrument(info_span!("AltData")));
    actors.spawn(market_data.run().instrument(info_span!("MarketData")));
    actors.spawn(strat.run().instrument(info_span!("Strat")));
    actors.spawn(exec.run().instrument(info_span!("Exec")));

    info!("Waiting for actors");

    tokio::select! {
        _ = async {
             while let Some(res) = actors.join_next().await {
                 match res {
                    Ok(Ok(()))  => info!("Actor exited cleanly"),
                    Ok(Err(e))  => error!(?e, "Actor returned error"),
                    Err(panic)  => error!(?panic, "Actor panicked/cancelled"),
                }
            }
        } => {  }
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl-C received, shutting down supervisor loop");
            shutdown.cancel();
        }
    }

    info!("Waiting for graceful shutdown of actors");
    while let Some(res) = actors.join_next().await {
        match res {
            Ok(Ok(()))  => info!("Actor exited cleanly"),
            Ok(Err(e))  => error!(?e, "Actor returned error"),
            Err(panic)  => error!(?panic, "Actor panicked/cancelled"),
        }
    }


    info!("Supervisor exit");
    Ok(())
}
