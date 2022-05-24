#![allow(unused_variables, dead_code)]

use std::sync::Arc;

use semver::Version;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::info;

use tendermint_rpc::endpoint::broadcast::tx_sync;

use ibc::{
    core::{
        ics02_client::{
            client_consensus::{AnyConsensusState, AnyConsensusStateWithHeight},
            client_state::{AnyClientState, IdentifiedAnyClientState},
            events::UpdateClient,
            misbehaviour::MisbehaviourEvidence,
        },
        ics03_connection::connection::{ConnectionEnd, IdentifiedConnectionEnd},
        ics04_channel::{
            channel::{ChannelEnd, IdentifiedChannelEnd},
            packet::{PacketMsgType, Sequence},
        },
        ics23_commitment::{commitment::CommitmentPrefix, merkle::MerkleProof},
        ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId},
    },
    events::IbcEvent,
    query::{QueryBlockRequest, QueryTxRequest},
    Height,
};

use crate::{
    account::Balance,
    chain::{
        client::ClientSettings,
        endpoint::{ChainEndpoint, ChainStatus, HealthCheck},
        requests::*,
        tracking::TrackedMsgs,
    },
    config::ChainConfig,
    error::Error,
    event::monitor::{EventReceiver, TxMonitorCmd},
    keyring::{KeyEntry, KeyRing},
    light_client::{tendermint::LightClient as TmLightClient, LightClient, Verified},
};

use super::CosmosSdkChain;

flex_error::define_error! {
    PsqlError {
        MissingConnectionConfig
            { chain_id: ChainId }
            |e| { format_args!("missing `psql_conn` config for chain '{}'", e.chain_id) }
    }
}

pub struct PsqlChain {
    chain: CosmosSdkChain,
    pool: PgPool,
    rt: Arc<tokio::runtime::Runtime>,
}

impl ChainEndpoint for PsqlChain {
    type LightBlock = <CosmosSdkChain as ChainEndpoint>::LightBlock;

    type Header = <CosmosSdkChain as ChainEndpoint>::Header;

    type ConsensusState = <CosmosSdkChain as ChainEndpoint>::ConsensusState;

    type ClientState = <CosmosSdkChain as ChainEndpoint>::ClientState;

    type LightClient = PsqlLightClient;

    fn bootstrap(config: ChainConfig, rt: Arc<tokio::runtime::Runtime>) -> Result<Self, Error> {
        info!("bootsrapping");

        let psql_conn = config
            .psql_conn
            .as_deref()
            .ok_or_else(|| PsqlError::missing_connection_config(config.id.clone()))?;

        let pool = rt
            .block_on(PgPoolOptions::new().max_connections(5).connect(psql_conn))
            .map_err(Error::sqlx)?;

        info!("instantiating chain");

        let chain = CosmosSdkChain::bootstrap(config, rt.clone())?;

        Ok(Self { chain, pool, rt })
    }

    fn init_light_client(&self) -> Result<Self::LightClient, Error> {
        self.chain.init_light_client().map(PsqlLightClient)
    }

    fn init_event_monitor(
        &self,
        rt: Arc<tokio::runtime::Runtime>,
    ) -> Result<(EventReceiver, TxMonitorCmd), Error> {
        self.chain.init_event_monitor(rt)
    }

    fn id(&self) -> &ChainId {
        // let _ = &self.pool;
        // let _ = &self.rt;
        self.chain.id()
    }

    fn shutdown(self) -> Result<(), Error> {
        self.chain.shutdown()
    }

    fn health_check(&self) -> Result<HealthCheck, Error> {
        // TODO(romac): Check database connection

        self.chain.health_check()
    }

    fn keybase(&self) -> &KeyRing {
        self.chain.keybase()
    }

    fn keybase_mut(&mut self) -> &mut KeyRing {
        self.chain.keybase_mut()
    }

    fn send_messages_and_wait_commit(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<IbcEvent>, Error> {
        self.chain.send_messages_and_wait_commit(tracked_msgs)
    }

    fn send_messages_and_wait_check_tx(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<tx_sync::Response>, Error> {
        self.chain.send_messages_and_wait_check_tx(tracked_msgs)
    }

    fn get_signer(&mut self) -> Result<ibc::signer::Signer, Error> {
        self.chain.get_signer()
    }

    fn config(&self) -> ChainConfig {
        ChainEndpoint::config(&self.chain)
    }

    fn get_key(&mut self) -> Result<KeyEntry, Error> {
        self.chain.get_key()
    }

    fn add_key(&mut self, key_name: &str, key: KeyEntry) -> Result<(), Error> {
        self.chain.add_key(key_name, key)
    }

    fn ibc_version(&self) -> Result<Option<Version>, Error> {
        self.chain.ibc_version()
    }

    fn query_balance(&self) -> Result<Balance, Error> {
        self.chain.query_balance()
    }

    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error> {
        self.chain.query_commitment_prefix()
    }

    fn query_application_status(&self) -> Result<ChainStatus, Error> {
        self.chain.query_application_status()
    }

    fn query_clients(
        &self,
        request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error> {
        self.chain.query_clients(request)
    }

    fn query_client_state(
        &self,
        request: QueryClientStateRequest,
    ) -> Result<AnyClientState, Error> {
        self.chain.query_client_state(request)
    }

    fn query_consensus_states(
        &self,
        request: QueryConsensusStatesRequest,
    ) -> Result<Vec<AnyConsensusStateWithHeight>, Error> {
        self.chain.query_consensus_states(request)
    }

    fn query_consensus_state(
        &self,
        request: QueryConsensusStateRequest,
    ) -> Result<AnyConsensusState, Error> {
        self.chain.query_consensus_state(request)
    }

    fn query_upgraded_client_state(
        &self,
        request: QueryUpgradedClientStateRequest,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        self.chain.query_upgraded_client_state(request)
    }

    fn query_upgraded_consensus_state(
        &self,
        request: QueryUpgradedConsensusStateRequest,
    ) -> Result<(AnyConsensusState, MerkleProof), Error> {
        self.chain.query_upgraded_consensus_state(request)
    }

    fn query_connections(
        &self,
        request: QueryConnectionsRequest,
    ) -> Result<Vec<IdentifiedConnectionEnd>, Error> {
        self.chain.query_connections(request)
    }

    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
    ) -> Result<Vec<ConnectionId>, Error> {
        self.chain.query_client_connections(request)
    }

    fn query_connection(&self, request: QueryConnectionRequest) -> Result<ConnectionEnd, Error> {
        self.chain.query_connection(request)
    }

    fn query_connection_channels(
        &self,
        request: QueryConnectionChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        self.chain.query_connection_channels(request)
    }

    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        self.chain.query_channels(request)
    }

    fn query_channel(&self, request: QueryChannelRequest) -> Result<ChannelEnd, Error> {
        self.chain.query_channel(request)
    }

    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error> {
        self.chain.query_channel_client_state(request)
    }

    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        self.chain.query_packet_commitments(request)
    }

    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<Sequence>, Error> {
        self.chain.query_unreceived_packets(request)
    }

    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        self.chain.query_packet_acknowledgements(request)
    }

    fn query_unreceived_acknowledgements(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<Sequence>, Error> {
        self.chain.query_unreceived_acknowledgements(request)
    }

    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
    ) -> Result<Sequence, Error> {
        self.chain.query_next_sequence_receive(request)
    }

    fn query_txs(&self, request: QueryTxRequest) -> Result<Vec<IbcEvent>, Error> {
        self.chain.query_txs(request)
    }

    fn query_blocks(
        &self,
        request: QueryBlockRequest,
    ) -> Result<(Vec<IbcEvent>, Vec<IbcEvent>), Error> {
        self.chain.query_blocks(request)
    }

    fn query_host_consensus_state(
        &self,
        request: QueryHostConsensusStateRequest,
    ) -> Result<Self::ConsensusState, Error> {
        self.chain.query_host_consensus_state(request)
    }

    fn proven_client_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        self.chain.proven_client_state(client_id, height)
    }

    fn proven_connection(
        &self,
        connection_id: &ConnectionId,
        height: Height,
    ) -> Result<(ConnectionEnd, MerkleProof), Error> {
        self.chain.proven_connection(connection_id, height)
    }

    fn proven_client_consensus(
        &self,
        client_id: &ClientId,
        consensus_height: Height,
        height: Height,
    ) -> Result<(AnyConsensusState, MerkleProof), Error> {
        self.chain
            .proven_client_consensus(client_id, consensus_height, height)
    }

    fn proven_channel(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: Height,
    ) -> Result<(ChannelEnd, MerkleProof), Error> {
        self.chain.proven_channel(port_id, channel_id, height)
    }

    fn proven_packet(
        &self,
        packet_type: PacketMsgType,
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
        height: Height,
    ) -> Result<(Vec<u8>, MerkleProof), Error> {
        self.chain
            .proven_packet(packet_type, port_id, channel_id, sequence, height)
    }

    fn build_client_state(
        &self,
        height: Height,
        settings: ClientSettings,
    ) -> Result<Self::ClientState, Error> {
        self.chain.build_client_state(height, settings)
    }

    fn build_consensus_state(
        &self,
        light_block: Self::LightBlock,
    ) -> Result<Self::ConsensusState, Error> {
        self.chain.build_consensus_state(light_block)
    }

    fn build_header(
        &self,
        trusted_height: Height,
        target_height: Height,
        client_state: &AnyClientState,
        light_client: &mut Self::LightClient,
    ) -> Result<(Self::Header, Vec<Self::Header>), Error> {
        self.chain.build_header(
            trusted_height,
            target_height,
            client_state,
            &mut light_client.0,
        )
    }
}

pub struct PsqlLightClient(TmLightClient);

impl LightClient<PsqlChain> for PsqlLightClient {
    fn header_and_minimal_set(
        &mut self,
        trusted: Height,
        target: Height,
        client_state: &AnyClientState,
    ) -> Result<Verified<<PsqlChain as ChainEndpoint>::Header>, Error> {
        self.0.header_and_minimal_set(trusted, target, client_state)
    }

    fn verify(
        &mut self,
        trusted: Height,
        target: Height,
        client_state: &AnyClientState,
    ) -> Result<Verified<<PsqlChain as ChainEndpoint>::LightBlock>, Error> {
        self.0.verify(trusted, target, client_state)
    }

    fn check_misbehaviour(
        &mut self,
        update: UpdateClient,
        client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        self.0.check_misbehaviour(update, client_state)
    }

    fn fetch(&mut self, height: Height) -> Result<<PsqlChain as ChainEndpoint>::LightBlock, Error> {
        self.0.fetch(height)
    }
}
