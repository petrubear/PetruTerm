use super::*;
use futures_util::{SinkExt, StreamExt};
use remote_proto::noise::{self, Mode};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn start(limits: Limits) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, limits));
    addr
}

async fn connect(addr: std::net::SocketAddr, role: &str, id: &str) -> Client {
    connect_async(format!("ws://{addr}/{role}/{id}"))
        .await
        .unwrap()
        .0
}

async fn next_binary(ws: &mut Client) -> Vec<u8> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out")
        {
            Some(Ok(Message::Binary(b))) => return b.to_vec(),
            Some(Ok(Message::Close(_))) | None => panic!("closed"),
            Some(Ok(_)) => continue,
            Some(Err(e)) => panic!("{e}"),
        }
    }
}

async fn expect_closed(ws: &mut Client) {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for close")
        {
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
            Some(Ok(_)) => continue,
        }
    }
}

const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn conn_of(frame: &[u8]) -> [u8; 8] {
    frame[1..9].try_into().unwrap()
}

#[tokio::test]
async fn forwards_both_ways_with_connection_ids() {
    let addr = start(Limits::default()).await;
    let mut host = connect(addr, "host", ID).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut phone = connect(addr, "client", ID).await;

    let new = next_binary(&mut host).await;
    assert_eq!(new[0], TO_HOST_NEW);
    let conn = conn_of(&new);

    phone
        .send(Message::Binary(b"ping".to_vec().into()))
        .await
        .unwrap();
    let data = next_binary(&mut host).await;
    assert_eq!(data[0], TO_HOST_DATA);
    assert_eq!(conn_of(&data), conn);
    assert_eq!(&data[9..], b"ping");

    let mut reply = vec![FROM_HOST_DATA];
    reply.extend(conn);
    reply.extend(b"pong");
    host.send(Message::Binary(reply.into())).await.unwrap();
    assert_eq!(next_binary(&mut phone).await, b"pong");

    drop(phone);
    let gone = next_binary(&mut host).await;
    assert_eq!(gone[0], TO_HOST_GONE);
    assert_eq!(conn_of(&gone), conn);
}

#[tokio::test]
async fn noise_session_works_through_the_relay() {
    let addr = start(Limits::default()).await;
    let host_kp = noise::generate_keypair().unwrap();
    let phone_kp = noise::generate_keypair().unwrap();
    let id = remote_proto::endpoint_id(&host_kp.public);
    let mut host = connect(addr, "host", &id).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut phone = connect(addr, "client", &id).await;
    let conn = conn_of(&next_binary(&mut host).await);

    let (init, first) = noise::initiate(&phone_kp, &host_kp.public, Mode::Reconnect).unwrap();
    phone.send(Message::Binary(first.into())).await.unwrap();
    let frame = next_binary(&mut host).await;
    let resp = noise::respond_read(&host_kp, &frame[9..], None).unwrap();
    assert_eq!(resp.remote_static(), phone_kp.public);
    let (reply, mut host_session) = resp.reply().unwrap();
    let mut out = vec![FROM_HOST_DATA];
    out.extend(conn);
    out.extend(reply);
    host.send(Message::Binary(out.into())).await.unwrap();
    let mut phone_session = init.finish(&next_binary(&mut phone).await).unwrap();

    let frames = phone_session.seal(b"secret input").unwrap();
    phone
        .send(Message::Binary(frames[0].clone().into()))
        .await
        .unwrap();
    let got = next_binary(&mut host).await;
    assert_eq!(
        host_session.open(&got[9..]).unwrap().unwrap(),
        b"secret input"
    );
    // The relay never saw the plaintext.
    assert!(!got.windows(6).any(|w| w == b"secret"));
}

#[tokio::test]
async fn client_to_offline_host_is_refused() {
    let addr = start(Limits::default()).await;
    let mut phone = connect(addr, "client", ID).await;
    expect_closed(&mut phone).await;
}

#[tokio::test]
async fn newer_host_replaces_older_and_drops_its_clients() {
    let addr = start(Limits::default()).await;
    let mut old = connect(addr, "host", ID).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut phone = connect(addr, "client", ID).await;
    next_binary(&mut old).await;
    let _new = connect(addr, "host", ID).await;
    expect_closed(&mut old).await;
    expect_closed(&mut phone).await;
}

#[tokio::test]
async fn bad_paths_are_rejected() {
    let addr = start(Limits::default()).await;
    assert!(connect_async(format!("ws://{addr}/host/short"))
        .await
        .is_err());
    assert!(connect_async(format!("ws://{addr}/admin/{ID}"))
        .await
        .is_err());
    assert!(connect_async(format!("ws://{addr}/")).await.is_err());
}

#[tokio::test]
async fn too_many_clients_per_host() {
    let limits = Limits {
        max_clients_per_host: 2,
        ..Limits::default()
    };
    let addr = start(limits).await;
    let mut host = connect(addr, "host", ID).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let _a = connect(addr, "client", ID).await;
    let _b = connect(addr, "client", ID).await;
    next_binary(&mut host).await;
    next_binary(&mut host).await;
    let mut c = connect(addr, "client", ID).await;
    expect_closed(&mut c).await;
}

#[tokio::test]
async fn oversized_and_text_frames_close_the_client() {
    let limits = Limits {
        max_frame: 1024,
        ..Limits::default()
    };
    let addr = start(limits).await;
    let _host = connect(addr, "host", ID).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut big = connect(addr, "client", ID).await;
    big.send(Message::Binary(vec![0u8; 4096].into()))
        .await
        .unwrap();
    expect_closed(&mut big).await;
    let mut text = connect(addr, "client", ID).await;
    text.send(Message::Text("hi".into())).await.unwrap();
    expect_closed(&mut text).await;
}

#[tokio::test]
async fn flooding_client_is_cut_off() {
    let limits = Limits {
        client_frames_per_sec: 5,
        ..Limits::default()
    };
    let addr = start(limits).await;
    let _host = connect(addr, "host", ID).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut phone = connect(addr, "client", ID).await;
    for _ in 0..50 {
        if phone.send(Message::Binary(vec![1].into())).await.is_err() {
            break;
        }
    }
    expect_closed(&mut phone).await;
}

#[tokio::test]
async fn connection_cap_per_ip() {
    let limits = Limits {
        max_connections_per_ip: 2,
        ..Limits::default()
    };
    let addr = start(limits).await;
    let _a = connect(addr, "host", ID).await;
    let _b = connect(addr, "client", ID).await;
    assert!(connect_async(format!("ws://{addr}/client/{ID}"))
        .await
        .is_err());
}

#[test]
fn bucket_refills() {
    let mut b = Bucket::new(2);
    assert!(b.take());
    assert!(b.take());
    assert!(!b.take());
}
