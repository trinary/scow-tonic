use scow::{scow_key_value_client::ScowKeyValueClient, StatusRequest, GetRequest, SetRequest};

pub mod scow {
    tonic::include_proto!("scow");
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = ScowKeyValueClient::connect("http://[::1]:50051").await?;
   // let mut client2 = ScowKeyValueClient::connect("http://[::1]:50052").await?;

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

    // let missing_key_response = client.get(
    //     tonic::Request::new(
    //         GetRequest { key: String::from("zzz")}
    //     )
    // ).await?;

    // println!("missing key get response: {:?}", missing_key_response);

    // let client2_get_response = client2.get(
    //     tonic::Request::new(
    //         GetRequest {key: String::from("abc")}
    //     )
    // ).await;

    // match client2_get_response {
    //     Ok(s) => println!("client2 got an OK response: {:?}", s),
    //     Err(e) => println!("client2 got an Erro response: {:?}", e),
    // };

    Ok(())
}