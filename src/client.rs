// Rust JSON-RPC Library
// Written in 2015 by
//     Andrew Poelstra <apoelstra@wpsoftware.net>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! # Client support
//!
//! Support for connecting to JSONRPC servers over HTTP, sending requests,
//! and parsing responses
//!
use std::sync::{Arc, Mutex};
use hyper::client::{Client as HyperClient, HttpConnector};
use hyper::{self, Body, Method, HeaderMap};
use hyper::header::{self, HeaderValue};
use futures::{future, Future, Stream};
use serde_json;
use serde;
use super::{Request, Response};
use error::Error;
use std::time::{Duration, Instant};
use std::ops::Add;
use tokio::timer::Timeout;

use std::sync::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};


pub const VERSION:&str = "0.11.14";

lazy_static! {
    static ref _HyperClient: RwLock<HyperClient<HttpConnector, Body>> = RwLock::new({HyperClient::new()});
}

fn get_hyper_client() -> HyperClient<HttpConnector, Body>{
    match _HyperClient.read(){
        Ok(cli) => {
            (*cli).clone()
        },
        _ => HyperClient::new(),
    }
}

//lazy_static! {
//    static ref  _HyperClient:HyperClient<HttpConnector, Body> = HyperClient::new();
//}
//
//fn get_hyper_client() -> HyperClient<HttpConnector, Body>{
//    _HyperClient.clone()
//}


lazy_static! {
    static ref _msg_id:Arc<AtomicUsize> = Arc::new(AtomicUsize::new(1));
}

fn get_msg_id() -> usize{
    return _msg_id.fetch_add(1, Ordering::SeqCst);
}

/// A handle to a remote JSONRPC server
#[derive(Clone, Debug)]
pub struct Client {
    url: String,
//    user: Option<String>,
//    pass: Option<String>,
    client: HyperClient<HttpConnector, Body>,
    //nonce: Arc<Mutex<u64>>,
    timeout: Option<Duration>,
    ignore_nonce_mismatch: bool,
}


impl Client {
    /// Creates a new client
    pub fn new(url: String) -> Client {
        // Check that if we have a password, we have a username; other way around is ok
        //debug_assert!(pass.is_none() || user.is_some());

        Client {
            url: url,
            client: HyperClient::new(),
            //client: get_hyper_client(),
            //nonce: Arc::new(Mutex::new(0)),
            timeout: None,
            ignore_nonce_mismatch: false,
        }
    }
    
    ///Ignore NonceMismatch
    pub fn set_ignore_nonce_mismatch(&mut self, b:bool){
        self.ignore_nonce_mismatch = b;
    }
    
    ///set timeout
    pub fn set_timeout(&mut self, timeout : Duration) -> &Self{
        self.timeout = Some(timeout);
        self
    }

    /// Make a request and deserialize the response
    pub fn do_rpc<T>(&self, rpc_name: &str, args: &[serde_json::value::Value], headers: Option<HeaderMap<HeaderValue>>) 
    -> Box<Future<Item=T, Error=Error> + Send> 
    where T: Send , T: serde::de::DeserializeOwned, T : 'static { 
        let request = self.build_request(rpc_name, args);
        let response = self.send_request(&request, headers);
        Box::new(response.and_then(|res|{
            Ok(res.into_result::<T>()?)
        }))
    }


    /// Sends a request to a client
    pub fn send_request(&self, request: &Request, headers: Option<HeaderMap<HeaderValue>>) -> Box<Future<Item=Response, Error=Error> + Send> {
        let resp_fut = self.send_request_(request, headers);
        if self.timeout.is_some(){
            let deadline = Instant::now().add(self.timeout.unwrap());
            Box::new(Timeout::new_at(resp_fut, deadline).map_err(|e|{
                match e.into_inner(){
                    Some(r) =>  r,
                    None => Error::Timeout
                }
            }))
        }else{
            resp_fut
        }
    }

    /// rpc request header
    //pub headers: HeaderMap<HeaderValue>,

    fn send_request_(&self, request: &Request, headers: Option<HeaderMap<HeaderValue>>) -> Box<Future<Item=Response, Error=Error> + Send> {
        // Build request
        let request_raw = serde_json::to_vec(request);
        if request_raw.is_err(){
            return Box::new(future::err(Error::Json(request_raw.err().unwrap())));
        }
        let request_raw = request_raw.unwrap();
        let request_raw_clone = request_raw.clone();

        // Send request
        let mut hyper_request = hyper::Request::builder()
            .method(Method::POST)
            .uri(self.url.clone())
            .body(Body::from(request_raw))
            .unwrap();
            if headers.is_some(){
                *hyper_request.headers_mut() = headers.unwrap();
            }
        
        let msg_id = request.id.clone();
        let resp_fut = self.client.request(hyper_request);
        let ignore_nonce_mismatch = self.ignore_nonce_mismatch;
        
        //println!("request_raw_clone:{:?}", String::from_utf8(request_raw_clone));
        
        Box::new(resp_fut.and_then(|res| {
            res.into_body().concat2()
        }).then(move |body|{
            match body{
                Ok(b) =>{
                    match serde_json::from_slice::<Response>(&b){
                        Ok(response) => {
                            if response.jsonrpc != None && response.jsonrpc != Some(From::from("2.0")) {
                                return Err(Error::VersionMismatch);
                            }
                            if response.id != msg_id &&  !ignore_nonce_mismatch{
                                return Err(Error::NonceMismatch);
                            }
                            return Ok(response)
                        },
                        Err(e) => {
                            println!("{}, body:{:?}, err:{:?}, request_raw:{:?}", b.len(), b, e, String::from_utf8(request_raw_clone));
                            return Err(Error::Json(e))
                        },
                    }
                }
                Err(e) => return Err(Error::Hyper(e))
            }
        }))
    }

    /// Builds a request
    pub fn build_request<'a, 'b>(
        &self,
        name: &'a str,
        params: &'b [serde_json::Value],
    ) -> Request<'a, 'b> {
        //let mut nonce = self.nonce.lock().unwrap();
        //*nonce += 1;
        Request {
            method: name,
            params: params,
            //id: From::from(*nonce),
            id: From::from(get_msg_id()),
            jsonrpc: Some("2.0"),
        }
    }

    // / Accessor for the last-used nonce
//    pub fn last_nonce(&self) -> u64 {
//        *self.nonce.lock().unwrap()
//    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use std::thread::sleep;
    use tokio::timer::Timeout;
    use std::ops::Add;
    //use std::ops::Sub;

//    #[test]
//    fn call(){
//        call2();
//    }
    #[test]
    fn call(){
        
        //do_rpc<T>(&self, rpc_name: &str, args: &[serde_json::value::Value], headers: Option<HeaderMap<HeaderValue>>) 
        
        let fut1 = call2().and_then(|res|{
            println!("1 response:{:?}", res);
            Ok(())
        }).map_err(|e|{
            println!("e:{:?}", e);
        });
        
        let fut2 = call2().and_then(|res|{
            println!("2 response:{:?}", res);
            Ok(())
        }).map_err(|e|{
            println!("e:{:?}", e);
        });

        let fut3 = call2().and_then(|res|{
            println!("3 response:{:?}", res);
            Ok(())
        }).map_err(|e|{
            println!("e:{:?}", e);
        });

        let fut = fut1.join(fut2).join(fut3).and_then(|res|{
            println!("j response:{:?}", res);
            Ok(())
        }).map_err(|e|{
            println!("e:{:?}", e);
        });

//        let sp1 = hyper::rt::spawn(fut1);
//        let sp2 = hyper::rt::spawn(fut2);
        
        hyper::rt::run(fut);
    }
    
    fn call2()  -> Box<Future<Item=Response, Error=Error> + Send> {
        
       let deadline = Instant::now().add(Duration::from_millis(1010));
        
        
        let mut client = Client::new("http://app-backend-process-service.test.shouqianba.com/rpc/account".to_owned());
        client.set_timeout(Duration::from_millis(1000));
        
        let params = vec![json!("94ca4d57-a610-49d1-b93c-a30e725ea40b".to_owned())];
        
        let request = client.build_request("getAccount", &params); 
        
        let mut headers = HeaderMap::new();
        //headers.insert(AUTHORIZATION, user.clone().parse().unwrap());
        //headers.insert("Connection", HeaderValue::from_static("close")); 
        headers.insert("Connection", HeaderValue::from_static("keep-alive")); 
        
        let response = client.send_request(&request, Some(headers));
        
        //let response = Timeout::new_at(response, deadline);
        
//        let fut = response.and_then(|res|{
//            println!("response:{:?}", res);
//            Ok(())
//        }).map_err(|e|{
//            println!("e:{:?}", e);
//        });
        
        let server_dt1 = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let now2 = Instant::now();
        println!("1 Duration::now():{:?}", server_dt1);
        //hyper::rt::run(fut);
        
        let server_dt2 = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        println!("2 Duration::now():{:?}, {:?}, {:?}", server_dt2 - server_dt1, now2.elapsed().as_secs(), now2.elapsed().subsec_millis());
        response
    }

//    #[test]
//    fn sanity() {
//        let client = Client::new("localhost".to_owned());
//        assert_eq!(client.last_nonce(), 0);
//        let req1 = client.build_request("test", &[]);
//        assert_eq!(client.last_nonce(), 1);
//        let req2 = client.build_request("test", &[]);
//        assert_eq!(client.last_nonce(), 2);
//        assert!(req1 != req2);
//    }
}
