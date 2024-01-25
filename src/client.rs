use scow::{scow_key_value_client::ScowKeyValueClient, StatusRequest, GetRequest, SetRequest};

pub mod scow {
    tonic::include_proto!("scow");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = ScowKeyValueClient::connect("http://[::1]:50051").await?;
    let request = tonic::Request::new(
        StatusRequest { }
    );
    let response = client.status(request).await?;
    println!("Status response: {:?}", response);

    let set_request = tonic::Request::new(
        SetRequest {
            key: String::from("abc"),
            value: String::from("abc value"),
        }
    );

    let set_response = client.set(set_request).await?;
    println!("set response: {:?}", set_response);

    let get_request = tonic::Request::new(
        GetRequest {
            key: String::from("abc"),
        }
    );

    let get_response = client.get(get_request).await?;
    println!("get response: {:?}", get_response);

    Ok(())
}