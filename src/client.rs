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
use hyper::{self, Body, Method};
use futures::{future, Future, Stream};
use serde_json;
use serde;
use super::{Request, Response};
use error::Error;

/// A handle to a remote JSONRPC server
pub struct Client {
    url: String,
//    user: Option<String>,
//    pass: Option<String>,
    client: HyperClient<HttpConnector, Body>,
    nonce: Arc<Mutex<u64>>,
}


impl Client {
    /// Creates a new client
    pub fn new(url: String) -> Client {
        // Check that if we have a password, we have a username; other way around is ok
        //debug_assert!(pass.is_none() || user.is_some());

        Client {
            url: url,
            client: HyperClient::new(),
            nonce: Arc::new(Mutex::new(0)),
        }
    }

    /// Make a request and deserialize the response
    pub fn do_rpc<T>(
        &self,
        rpc_name: &str,
        args: &[serde_json::value::Value],
    ) -> Box<Future<Item=T, Error=Error> + Send> 
    where T: Send , T: serde::de::DeserializeOwned, T : 'static { 
        let request = self.build_request(rpc_name, args);
        let response = self.send_request(&request);
        Box::new(response.and_then(|res|{
            Ok(res.into_result::<T>()?)
        }))
    }

    /// Sends a request to a client
    pub fn send_request(&self, request: &Request) -> Box<Future<Item=Response, Error=Error> + Send> {
        // Build request
        let request_raw = serde_json::to_vec(request);
        if request_raw.is_err(){
            return Box::new(future::err(Error::Json(request_raw.err().unwrap())));
        }
        let request_raw = request_raw.unwrap();

        // Setup connection
//        let mut headers = HeaderMap::new();
//        if let Some(ref user) = self.user {
//            headers.insert(AUTHORIZATION, user.clone().parse().unwrap());
//        }

        // Send request
        let hyper_request = hyper::Request::builder()
            .method(Method::POST)
            .uri(self.url.clone())
            .body(Body::from(request_raw))
            .unwrap();
            //*hyper_request.headers_mut() = headers;
        
        let msg_id = request.id.clone();
        let resp_fut = self.client.request(hyper_request);
        
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
                            if response.id != msg_id {
                                return Err(Error::NonceMismatch);
                            }
                            return Ok(response)
                        },
                        Err(e) => {
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
        let mut nonce = self.nonce.lock().unwrap();
        *nonce += 1;
        Request {
            method: name,
            params: params,
            id: From::from(*nonce),
            jsonrpc: Some("2.0"),
        }
    }

    /// Accessor for the last-used nonce
    pub fn last_nonce(&self) -> u64 {
        *self.nonce.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanity() {
        let client = Client::new("localhost".to_owned(), None, None);
        assert_eq!(client.last_nonce(), 0);
        let req1 = client.build_request("test", &[]);
        assert_eq!(client.last_nonce(), 1);
        let req2 = client.build_request("test", &[]);
        assert_eq!(client.last_nonce(), 2);
        assert!(req1 != req2);
    }
}
