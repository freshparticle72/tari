//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::support::{
    factories::{self, TestFactory},
    helpers::{streams::stream_assert_count, ConnectionMessageCounter},
};
use futures::channel::mpsc::channel;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tari_comms::{
    connection::{CurveEncryption, Direction, NetAddress, ZmqContext},
    connection_manager::{establisher::ConnectionEstablisher, ConnectionManagerError, PeerConnectionConfig},
    control_service::messages::{MessageHeader, MessageType, PongMessage},
    message::{Envelope, MessageExt, MessageFlags},
    utils::crypt,
    wrap_in_envelope_body,
};
use tari_storage::{
    lmdb_store::{LMDBBuilder, LMDBError, LMDBStore},
    LMDBWrapper,
};
use tari_utilities::thread_join::ThreadJoinWithTimeout;

fn make_peer_connection_config() -> PeerConnectionConfig {
    PeerConnectionConfig {
        peer_connection_establish_timeout: Duration::from_secs(5),
        max_message_size: 1024,
        max_connections: 10,
        host: "127.0.0.1".parse().unwrap(),
        max_connect_retries: 3,
        socks_proxy_address: None,
    }
}

fn get_path(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data");
    path.push(name);
    path.to_str().unwrap().to_string()
}

fn init_datastore(name: &str) -> Result<LMDBStore, LMDBError> {
    let path = get_path(name);
    let _ = std::fs::create_dir(&path).unwrap_or_default();
    LMDBBuilder::new()
        .set_path(&path)
        .set_environment_size(10)
        .set_max_number_of_databases(2)
        .add_database(name, lmdb_zero::db::CREATE)
        .build()
}

fn clean_up_datastore(name: &str) {
    std::fs::remove_dir_all(get_path(name)).unwrap();
}

// This tries to break the establisher by sending malformed messages. The establisher should
// disregard the malformed message and continue to try other addresses. Once all
// addresses fail, the correct error should be returned.
#[test]
fn establish_control_service_connection_fail() {
    let context = ZmqContext::new();

    let node_identity = factories::node_identity::create().build().map(Arc::new).unwrap();

    let peers = factories::peer::create_many(2)
        .with_factory(factories::peer::create().with_net_addresses_factory(factories::net_address::create_many(2)))
        .build()
        .unwrap();

    // Setup a connection counter to act as a 'junk' endpoint for a peers control service.
    let mut msg_counter1 = ConnectionMessageCounter::new(&context);
    msg_counter1.set_response(vec!["JUNK".as_bytes().to_vec()]);
    msg_counter1.start(peers[0].addresses[0].net_address.clone());

    let mut msg_counter2 = ConnectionMessageCounter::new(&context);
    msg_counter2.set_response(vec!["JUNK".as_bytes().to_vec()]);
    msg_counter2.start(peers[0].addresses[1].net_address.clone());

    // Note: every test should have unique database
    let database_name = "establisher_establish_control_service_connection_fail";
    let datastore = init_datastore(database_name).unwrap();
    let database = datastore.get_handle(database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_database(database)
            .with_peers(peers.clone())
            .build()
            .unwrap(),
    );
    let (tx, _rx) = channel(10);
    let config = make_peer_connection_config();

    let example_peer = &peers[0];

    let establisher = ConnectionEstablisher::new(context.clone(), node_identity, config, peer_manager, tx);
    match establisher.connect_control_service_client(example_peer) {
        Ok(_) => panic!("Unexpected success result"),
        Err(ConnectionManagerError::ControlServiceFailedConnectionAllAddresses) => {},
        Err(err) => panic!("Unexpected error type: {:?}", err),
    }

    msg_counter1.assert_count(1, 20);
    msg_counter2.assert_count(1, 20);

    clean_up_datastore(database_name);
}

#[test]
fn establish_control_service_connection_succeed() {
    let context = ZmqContext::new();
    let address = factories::net_address::create().build().unwrap();
    // The node attempting to connect
    let node_identity1 = factories::node_identity::create().build().map(Arc::new).unwrap();
    // The node being connected to
    let node_identity2 = factories::node_identity::create().build().map(Arc::new).unwrap();

    let example_peer = factories::peer::create()
        .with_public_key(node_identity2.identity.public_key.clone())
        .with_net_addresses(vec![address])
        .build()
        .unwrap();

    // Setup a connection counter to act as a control service sending back a pong
    let pong_response = {
        let body = wrap_in_envelope_body!(MessageHeader::new(MessageType::Pong), PongMessage {})
            .unwrap()
            .to_encoded_bytes()
            .unwrap();

        let shared_secret = crypt::generate_ecdh_secret(node_identity2.secret_key(), node_identity1.public_key());
        let encrypted_body = crypt::encrypt(&shared_secret, &body).unwrap();

        let envelope = Envelope::construct_signed(
            node_identity1.secret_key(),
            node_identity1.public_key(),
            encrypted_body,
            MessageFlags::ENCRYPTED,
        )
        .unwrap();
        envelope.to_encoded_bytes().unwrap()
    };

    let mut msg_counter1 = ConnectionMessageCounter::new(&context);
    msg_counter1.set_response(vec![pong_response]);

    let address = example_peer.addresses[0].net_address.clone();
    msg_counter1.start(address);

    // Setup peer manager
    let database_name = "establisher_establish_control_service_connection_succeed"; // Note: every test should have unique database
    let datastore = init_datastore(database_name).unwrap();
    let database = datastore.get_handle(database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_database(database)
            .with_peers(vec![example_peer.clone()])
            .build()
            .unwrap(),
    );
    let (tx, _rx) = channel(10);
    let config = make_peer_connection_config();
    let establisher = ConnectionEstablisher::new(context.clone(), node_identity1, config, peer_manager, tx);
    let client = establisher.connect_control_service_client(&example_peer).unwrap();
    client.ping_pong(Duration::from_millis(3000)).unwrap();

    msg_counter1.assert_count(2, 20);

    clean_up_datastore(database_name);
}

#[test]
fn establish_peer_connection_outbound() {
    let context = ZmqContext::new();
    let node_identity = factories::node_identity::create().build().map(Arc::new).unwrap();
    let (tx_inbound, rx_inbound) = channel(10);
    // Setup a peer connection
    let (peer_curve_sk, peer_curve_pk) = CurveEncryption::generate_keypair().unwrap();
    let (other_peer_conn, other_peer_conn_handle) = factories::peer_connection::create()
        .with_peer_connection_context_factory(
            factories::peer_connection_context::create()
                .with_message_sink_channel(tx_inbound)
                .with_curve_keypair((peer_curve_sk, peer_curve_pk.clone()))
                .with_context(&context)
                .with_direction(Direction::Inbound),
        )
        .build()
        .unwrap();

    other_peer_conn
        .wait_listening_or_failure(&Duration::from_millis(2000))
        .unwrap();

    let address = other_peer_conn.get_connected_address().unwrap().to_string();
    assert_ne!(address, "127.0.0.1:0");
    let address: NetAddress = other_peer_conn.get_connected_address().unwrap().into();

    let example_peer = factories::peer::create()
        .with_net_addresses(vec![address.clone()])
        .build()
        .unwrap();

    let database_name = "establisher_establish_peer_connection_outbound"; // Note: every test should have unique database
    let datastore = init_datastore(database_name).unwrap();
    let database = datastore.get_handle(database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_database(database)
            .with_peers(vec![example_peer.clone()])
            .build()
            .unwrap(),
    );
    let (tx_outbound2, _rx_outbound) = channel(10);
    let config = make_peer_connection_config();
    let establisher = ConnectionEstablisher::new(context.clone(), node_identity, config, peer_manager, tx_outbound2);
    let (connection, peer_conn_handle) = establisher
        .establish_outbound_peer_connection(example_peer.node_id.clone().into(), address, peer_curve_pk)
        .unwrap();
    connection.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
    connection.send(vec!["TARI".as_bytes().to_vec()]).unwrap();

    connection.shutdown().unwrap();
    connection.wait_disconnected(&Duration::from_millis(3000)).unwrap();

    other_peer_conn.shutdown().unwrap();
    other_peer_conn.wait_disconnected(&Duration::from_millis(3000)).unwrap();
    other_peer_conn_handle
        .timeout_join(Duration::from_millis(3000))
        .unwrap();

    let (_, _messages) = stream_assert_count(rx_inbound, 2, 500).unwrap();

    peer_conn_handle.timeout_join(Duration::from_millis(3000)).unwrap();

    clean_up_datastore(database_name);
}

#[test]
fn establish_peer_connection_inbound() {
    let context = ZmqContext::new();
    let node_identity = factories::node_identity::create().build().map(Arc::new).unwrap();

    let (secret_key, public_key) = CurveEncryption::generate_keypair().unwrap();

    let example_peer = factories::peer::create().build().unwrap();

    let database_name = "establish_peer_connection_inbound"; // Note: every test should have unique database
    let datastore = init_datastore(database_name).unwrap();
    let database = datastore.get_handle(database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_database(database)
            .with_peers(vec![example_peer.clone()])
            .build()
            .unwrap(),
    );

    let (tx, rx) = channel(10);
    // Create a connection establisher
    let config = make_peer_connection_config();
    let establisher = ConnectionEstablisher::new(context.clone(), node_identity, config, peer_manager, tx);
    let (connection, peer_conn_handle) = establisher
        .establish_inbound_peer_connection(example_peer.node_id.clone().into(), secret_key)
        .unwrap();

    connection
        .wait_listening_or_failure(&Duration::from_millis(3000))
        .unwrap();
    let address: NetAddress = connection.get_connected_address().unwrap().into();

    // Setup a peer connection which will connect to our established inbound peer connection
    let (other_tx, _other_rx) = channel(10);
    let (other_peer_conn, other_peer_conn_handle) = factories::peer_connection::create()
        .with_peer_connection_context_factory(
            factories::peer_connection_context::create()
                .with_context(&context)
                .with_server_public_key(public_key.clone())
                .with_direction(Direction::Outbound)
                .with_message_sink_channel(other_tx)
                .with_address(address),
        )
        .build()
        .unwrap();

    other_peer_conn
        .wait_connected_or_failure(&Duration::from_millis(3000))
        .unwrap();
    // Start sending messages

    other_peer_conn.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
    other_peer_conn.send(vec!["TARI".as_bytes().to_vec()]).unwrap();
    let _ = other_peer_conn.shutdown();
    other_peer_conn.wait_disconnected(&Duration::from_millis(3000)).unwrap();

    let (_arc_rx, _items) = stream_assert_count(rx, 2, 500).unwrap();

    peer_conn_handle.timeout_join(Duration::from_millis(3000)).unwrap();
    other_peer_conn_handle
        .timeout_join(Duration::from_millis(3000))
        .unwrap();

    clean_up_datastore(database_name);
}
