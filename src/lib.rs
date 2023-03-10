pub mod logger {
    use log::{Level, Metadata, Record};
    struct SimpleLogger;

    impl log::Log for SimpleLogger {
        fn enabled(&self, metadata: &Metadata) -> bool {
            metadata.level() <= Level::Info
        }

        fn log(&self, record: &Record) {
            if self.enabled(record.metadata()) {
                println!("{} - {}", record.level(), record.args());
            }
        }

        fn flush(&self) {}
    }

    use log::{LevelFilter, SetLoggerError};

    static LOGGER: SimpleLogger = SimpleLogger;

    pub fn init_logger() -> Result<(), SetLoggerError> {
        log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Info))
    }
}

pub mod file_logger {
    use chrono;
    use log::{LevelFilter, SetLoggerError};
    use log4rs::append::file::FileAppender;
    use log4rs::config::{Appender, Config, Root};
    use log4rs::encode::pattern::PatternEncoder;

    pub fn init_file_logger() -> Result<(), SetLoggerError> {
        let current_date = chrono::offset::Utc::now().date_naive().to_string();
        let path = format!("log/{}.log", current_date);

        let logfile = FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                "{d(%H:%M:%S)(utc)} {l} - {m}\n",
            )))
            .build(path);

        let config = Config::builder()
            .appender(Appender::builder().build("logfile", Box::new(logfile.unwrap())))
            .build(Root::builder().appender("logfile").build(LevelFilter::Info));

        log4rs::init_config(config.unwrap())?;

        Ok(())
    }
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub mod command {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    pub enum Command {
        createRoom { name: String },
        joinRoom { name: String, roomId: String },
        heartbeat {},
        startGame { token: String },
        getUserList { token: String },
        broadcastMessage { token: String, text: String },
    }

    #[derive(Serialize, Deserialize)]
    pub struct CommandTokenPair {
        #[serde(flatten)]
        command: Command,
        token: String,
    }
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub mod backend_models {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone)]
    pub struct User {
        pub id: String,
        pub name: String,
        pub avatarPath: String,
        pub roomId: String,
        pub isHost: bool,
    }

    #[derive(Serialize, Deserialize)]
    pub struct Room {
        pub id: String,
        pub max_players: i32,
        pub host_id: String,
        pub user_list: Vec<User>,
    }

    #[derive(Serialize, Deserialize)]
    pub enum Response {
        createRoomResponse { token: String },
        joinRoomResponse { token: String, userList: Vec<User> },
        updateUserList { userList: Vec<User> },
        newMessage { text: String },
        startGame {},
        errorReponse { errorText: String },
    }

    #[derive(Serialize, Deserialize)]
    pub struct Claims {
        pub id: String,
        pub roomId: String,
        pub exp: usize,
    }
}

pub mod server_messages {
    use crate::backend_models::{Response, User};
    use log::info;
    use std::{
        collections::HashMap,
        net::SocketAddr,
        sync::{Arc, Mutex},
    };

    use futures_channel::mpsc::UnboundedSender;
    use tungstenite::protocol::Message;

    type Tx = UnboundedSender<Message>;
    type PeerMap = Arc<Mutex<HashMap<(SocketAddr, String), Tx>>>;

    pub fn send_message(response: Response, peer_map: &PeerMap, addr: &SocketAddr) {
        info!("Sending msg to: {}", &addr);

        let peers = peer_map.lock().unwrap();
        let broadcast_recipients = peers
            .iter()
            .filter(|(peer_addr, _)| &peer_addr.0 == addr)
            .map(|(_, ws_sink)| ws_sink);

        for recp in broadcast_recipients {
            recp.unbounded_send(Message::Text(serde_json::to_string(&response).unwrap()))
                .unwrap();
        }
        info!("Message sent successfully to: {}", &addr);
    }
    pub fn broadcast_message_all(response: Response, peer_map: &PeerMap) {
        info!("Sending broadcast to all connections");
        let peers = peer_map.lock().unwrap();
        let broadcast_recipients = peers.iter().map(|(_, ws_sink)| ws_sink);

        for recp in broadcast_recipients {
            recp.unbounded_send(Message::Text(serde_json::to_string(&response).unwrap()))
                .unwrap();
        }
        info!("Broadcast sent successfully to all connections");
    }
    pub fn broadcast_message_except(response: Response, peer_map: &PeerMap, addr: &SocketAddr) {
        info!("Sending broadcast to all connections except: {}", &addr);
        let peers = peer_map.lock().unwrap();
        let broadcast_recipients = peers
            .iter()
            .filter(|(peer_addr, _)| &peer_addr.0 != addr)
            .map(|(_, ws_sink)| ws_sink);

        for recp in broadcast_recipients {
            recp.unbounded_send(Message::Text(serde_json::to_string(&response).unwrap()))
                .unwrap();
        }
        info!(
            "Broadcast sent successfully to all connections except: {}",
            &addr
        );
    }
    pub fn broadcast_message_room_all(
        response: Response,
        peer_map: &PeerMap,
        user_list: &Vec<User>,
    ) {
        info!("Sending broadcast to all room players");
        let peers = peer_map.lock().unwrap();
        let broadcast_recipients = peers
            .iter()
            .filter(|(peer_addr, _)| {
                user_list
                    .iter()
                    .map(|user| &user.id)
                    .any(|id| id == &peer_addr.1)
            })
            .map(|(_, ws_sink)| ws_sink);

        for recp in broadcast_recipients {
            recp.unbounded_send(Message::Text(serde_json::to_string(&response).unwrap()))
                .unwrap();
        }
        info!("Broadcast sent successfully to all room players");
    }
    pub fn broadcast_message_room_except(
        response: Response,
        peer_map: &PeerMap,
        user_list: &Vec<User>,
        addr: &SocketAddr,
    ) {
        info!("Sending broadcast to all room players except: {}", &addr);
        let peers = peer_map.lock().unwrap();
        let broadcast_recipients = peers
            .iter()
            .filter(|(peer_addr, _)| {
                user_list
                    .iter()
                    .map(|user| &user.id)
                    .any(|id| id == &peer_addr.1)
            } && &peer_addr.0 != addr)
            .map(|(_, ws_sink)| ws_sink);

        for recp in broadcast_recipients {
            recp.unbounded_send(Message::Text(serde_json::to_string(&response).unwrap()))
                .unwrap();
        }
        info!(
            "Broadcast sent successfully to all room players except: {}",
            &addr
        );
    }
}

pub mod jwtoken_generation {
    use chrono::{Days, Utc};
    use jsonwebtoken::{
        decode, encode, Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation,
    };

    use crate::backend_models::Claims;

    pub fn generate_token(
        id: &String,
        room_id: &String,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let expiration = Utc::now()
            .checked_add_days(Days::new(1))
            .expect("Timestamp invalid")
            .timestamp();

        let new_claims = Claims {
            id: id.clone(),
            roomId: room_id.clone(),
            exp: expiration as usize,
        };
        let token = encode(
            &Header::default(),
            &new_claims,
            &EncodingKey::from_secret("secret".as_ref()),
        );
        return token;
    }

    pub fn decode_token(token: &String) -> Result<TokenData<Claims>, jsonwebtoken::errors::Error> {
        return decode::<Claims>(
            &token,
            &DecodingKey::from_secret("secret".as_ref()),
            &Validation::new(Algorithm::HS256),
        );
    }
}
