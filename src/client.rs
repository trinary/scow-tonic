use scow::{scow_key_value_client::ScowKeyValueClient, GetRequest, SetRequest, StatusRequest};

use crate::scow::RequestVoteRequest;

pub mod scow {
    tonic::include_proto!("scow");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = ScowKeyValueClient::connect("http://[::1]:50051").await?;
    // let mut client2 = ScowKeyValueClient::connect("http://[::1]:50052").await?;

    let status_request = tonic::Request::new(StatusRequest {});
    let status_response = client.status(status_request).await;
    println!("Status response: {:?}", status_response);

    let get_request = tonic::Request::new(GetRequest {
        key: String::from("abc"),
    });

    let get_response = client.get(get_request).await;
    println!("get response: {:?}", get_response);

    let set_request = tonic::Request::new(SetRequest {
        key: String::from("abc"),
        value: String::from("abc value"),
    });

    let set_response = client.set(set_request).await;
    println!("set response: {:?}", set_response);

    let get_request2 = tonic::Request::new(GetRequest {
        key: String::from("abc"),
    });

    let get_response2 = client.get(get_request2).await;
    println!("get response2 : {:?}", get_response2);

    let vote_request = tonic::Request::new(RequestVoteRequest {
        term: 10,
        candidate_id: 11,
        last_log_index: 12,
        last_log_term: 13,
    });

    let vote_response = client.request_vote(vote_request).await;

    println!("vote response: {:?}", vote_response);
    Ok(())
}
