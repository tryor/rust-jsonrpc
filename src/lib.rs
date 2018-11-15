// Rust JSON-RPC Library
// Written in 2015 by
//   Andrew Poelstra <apoelstra@wpsoftware.net>
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

//! # Rust JSON-RPC Library
//!
//! Rust support for the JSON-RPC 2.0 protocol.
//!

#![crate_type = "lib"]
#![crate_type = "rlib"]
#![crate_type = "dylib"]
#![crate_name = "jsonrpc"]
// Coding conventions
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![warn(missing_docs)]

//#![feature(futures_api)] 

extern crate futures;
extern crate hyper;
extern crate tokio;
#[macro_use] extern crate lazy_static;
extern crate serde;
#[macro_use] extern crate serde_json;
#[macro_use] extern crate serde_derive;


pub mod client;
pub mod error;

// Re-export error type
pub use error::Error;

#[derive(Debug, Clone, PartialEq, Serialize)]
/// A JSONRPC request object
pub struct Request<'a, 'b> {
    /// The name of the RPC call
    pub method: &'a str,
    /// Parameters to the RPC call
    pub params: &'b [serde_json::Value],
    /// Identifier for this Request, which should appear in the response
    pub id: serde_json::Value,
    /// jsonrpc field, MUST be "2.0"
    pub jsonrpc: Option<&'a str>,
}

fn test(mut x: String) { x.push('o'); println!("{}", x); }
use futures::{future, Future};
impl<'a, 'b> Request<'a, 'b>  {
    fn new(params: &'static mut  serde_json::Value) -> Box<Future<Item=&'static mut serde_json::Value, Error=String> + Send> 
    { 
       //return params //Box::new(params);
       Box::new(future::ok(params))
    }
    
    fn test(mut x: String) { x.push('o'); println!("{}", x); }
}


#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
/// A JSONRPC response object
pub struct Response {
    /// A result if there is one, or null
    pub result: Option<serde_json::Value>,
    /// An error if there is one, or null
    pub error: Option<error::RpcError>,
    /// Identifier for this Request, which should match that of the request
    pub id: serde_json::Value,
    /// jsonrpc field, MUST be "2.0"
    pub jsonrpc: Option<String>,
}

impl Response {
    /// Extract the result from a response
    pub fn result<T: serde::de::DeserializeOwned>(&self) -> Result<T, Error> {
        if let Some(ref e) = self.error {
            return Err(Error::Rpc(e.clone()));
        }
        match self.result {
            Some(ref res) => serde_json::from_value(res.clone()).map_err(Error::Json),
            None => Err(Error::NoErrorOrResult),
        }
    }

    /// Extract the result from a response, consuming the response
    pub fn into_result<T: serde::de::DeserializeOwned>(self) -> Result<T, Error> {
        if let Some(e) = self.error {
            return Err(Error::Rpc(e));
        }

        match self.result {
            Some(ref res) => serde_json::from_value(res.clone()).map_err(Error::Json),
            None => Err(Error::NoErrorOrResult),
        }
    }

    /// Return the RPC error, if there was one, but do not check the result
    pub fn check_error(self) -> Result<(), Error> {
        if let Some(e) = self.error {
            Err(Error::Rpc(e))
        } else {
            Ok(())
        }
    }

    /// Returns whether or not the `result` field is empty
    pub fn is_none(&self) -> bool {
        self.result.is_none()
    }
}

#[cfg(test)]
mod tests {

    use super::Response;
    use serde_json;

    #[test]
    fn response_is_none() {
        let joanna = Response {
            result: Some(From::from(true)),
            error: None,
            id: From::from(81),
            jsonrpc: Some(String::from("2.0")),
        };

        let bill = Response {
            result: None,
            error: None,
            id: From::from(66),
            jsonrpc: Some(String::from("2.0")),
        };

        assert!(!joanna.is_none());
        assert!(bill.is_none());
    }

    #[test]
    fn response_extract() {
        let obj = vec!["Mary", "had", "a", "little", "lamb"];
        let response = Response {
            result: Some(serde_json::to_value(&obj).unwrap()),
            error: None,
            id: serde_json::Value::Null,
            jsonrpc: Some(String::from("2.0")),
        };
        let recovered1: Vec<String> = response.result().unwrap();
        assert!(response.clone().check_error().is_ok());
        let recovered2: Vec<String> = response.into_result().unwrap();
        assert_eq!(obj, recovered1);
        assert_eq!(obj, recovered2);
    }
}
