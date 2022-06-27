use async_trait::async_trait;

use crate::traits::message_sender::{IbcMessageSender, IbcMessageSenderExt, MessageSenderContext};
use crate::traits::messages::receive_packet::ReceivePacketMessageBuilder;
use crate::traits::packet_relayer::PacketRelayer;
use crate::traits::queries::status::{ChainStatus, ChainStatusQuerier};
use crate::traits::relay_types::{RelayContext, RelayTypes};
use crate::traits::target::DestinationTarget;
use crate::types::aliases::Packet;

pub struct ReceivePacketRelayer;

#[async_trait]
impl<Context, Relay, Error, Sender> PacketRelayer<Context> for ReceivePacketRelayer
where
    Relay: RelayTypes<Error = Error>,
    Context: RelayContext<RelayTypes = Relay, Error = Error>,
    Context: ReceivePacketMessageBuilder<Relay>,
    Context::SrcChainContext: ChainStatusQuerier<Relay::SrcChain>,
    Context: MessageSenderContext<DestinationTarget, Sender = Sender>,
    Sender: IbcMessageSender<Context, DestinationTarget>,
{
    type Return = ();

    async fn relay_packet(&self, context: &Context, packet: Packet<Relay>) -> Result<(), Error> {
        let source_height = context
            .source_context()
            .query_chain_status()
            .await?
            .height();

        let message = context
            .build_receive_packet_message(&source_height, &packet)
            .await?;

        context
            .message_sender()
            .send_message(context, message)
            .await?;

        Ok(())
    }
}
