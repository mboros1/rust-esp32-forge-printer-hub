use core::pin::pin;
use std::ffi::{c_char, CStr};
use embassy_futures::select::{select, Either};
use std::string::ToString;
use std::time::Duration;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::task;
use esp_idf_svc::mqtt::client::{EspAsyncMqttClient, EspAsyncMqttConnection, MqttClientConfiguration, QoS};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{EspError, ESP_ERR_INVALID_ARG};
use esp_idf_svc::timer::{EspAsyncTimer, EspTimerService, Task};
use esp_idf_svc::tls;
use esp_idf_svc::tls::X509;
use esp_idf_svc::wifi::{AsyncWifi, AuthMethod, BlockingWifi, ClientConfiguration, EspWifi};
use log::{error, info};
use uuid::Uuid;

const IP:&str = "192.168.4.92";
const SERIAL:&str = "03919C452004225";
const ACCESS_CODE:&str = "83057283";


const BBL_CA_PEM: &str = "
-----BEGIN CERTIFICATE-----
MIIDZTCCAk2gAwIBAgIUV1FckwXElyek1onFnQ9kL7Bk4N8wDQYJKoZIhvcNAQEL
BQAwQjELMAkGA1UEBhMCQ04xIjAgBgNVBAoMGUJCTCBUZWNobm9sb2dpZXMgQ28u
LCBMdGQxDzANBgNVBAMMBkJCTCBDQTAeFw0yMjA0MDQwMzQyMTFaFw0zMjA0MDEw
MzQyMTFaMEIxCzAJBgNVBAYTAkNOMSIwIAYDVQQKDBlCQkwgVGVjaG5vbG9naWVz
IENvLiwgTHRkMQ8wDQYDVQQDDAZCQkwgQ0EwggEiMA0GCSqGSIb3DQEBAQUAA4IB
DwAwggEKAoIBAQDL3pnDdxGOk5Z6vugiT4dpM0ju+3Xatxz09UY7mbj4tkIdby4H
oeEdiYSZjc5LJngJuCHwtEbBJt1BriRdSVrF6M9D2UaBDyamEo0dxwSaVxZiDVWC
eeCPdELpFZdEhSNTaT4O7zgvcnFsfHMa/0vMAkvE7i0qp3mjEzYLfz60axcDoJLk
p7n6xKXI+cJbA4IlToFjpSldPmC+ynOo7YAOsXt7AYKY6Glz0BwUVzSJxU+/+VFy
/QrmYGNwlrQtdREHeRi0SNK32x1+bOndfJP0sojuIrDjKsdCLye5CSZIvqnbowwW
1jRwZgTBR29Zp2nzCoxJYcU9TSQp/4KZuWNVAgMBAAGjUzBRMB0GA1UdDgQWBBSP
NEJo3GdOj8QinsV8SeWr3US+HjAfBgNVHSMEGDAWgBSPNEJo3GdOj8QinsV8SeWr
3US+HjAPBgNVHRMBAf8EBTADAQH/MA0GCSqGSIb3DQEBCwUAA4IBAQABlBIT5ZeG
fgcK1LOh1CN9sTzxMCLbtTPFF1NGGA13mApu6j1h5YELbSKcUqfXzMnVeAb06Htu
3CoCoe+wj7LONTFO++vBm2/if6Jt/DUw1CAEcNyqeh6ES0NX8LJRVSe0qdTxPJuA
BdOoo96iX89rRPoxeed1cpq5hZwbeka3+CJGV76itWp35Up5rmmUqrlyQOr/Wax6
itosIzG0MfhgUzU51A2P/hSnD3NDMXv+wUY/AvqgIL7u7fbDKnku1GzEKIkfH8hm
Rs6d8SCU89xyrwzQ0PR853irHas3WrHVqab3P+qNwR0YirL0Qk7Xt/q3O1griNg2
Blbjg3obpHo9
-----END CERTIFICATE-----
";

fn ca_pem_cstr() -> &'static CStr {
    // Create a null-terminated string at runtime
    // Box it and leak it to extend the lifetime to 'static
    let pem_with_nul = format!("{BBL_CA_PEM}\0");
    let boxed = pem_with_nul.into_boxed_str();
    let ptr = Box::leak(boxed).as_ptr() as *const c_char;
    unsafe {
        CStr::from_ptr(ptr)
    }
}


fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let timer_service = EspTimerService::new().unwrap();
    let modem = peripherals.modem;

    task::block_on(async {
        let wifi = wifi_create("chaos central",
                               "tinypotato392", modem, sysloop, &timer_service).await?;
        info!("Wifi created");

        let server_uri = format!("mqtts://{}:8883", IP);
        let client_id = format!("bbl-client-{}", Uuid::new_v4());
        let access_code = ACCESS_CODE;

        let (mut client, mut conn) = mqtt_create(&*server_uri, &*client_id, &*access_code)?;
        info!("MQTT client created");

        let mut timer = timer_service.timer_async()?;
        run(&mut client, &mut conn, &mut timer, &*format!("device/{SERIAL}/report")).await
    }).unwrap();

    info!("Hello, world!");
    Ok(())
}

async fn run(
    _client: &mut EspAsyncMqttClient, // not used anymore
    connection: &mut EspAsyncMqttConnection,
    _timer: &mut EspAsyncTimer, // not used anymore
    _topic: &str, // not used anymore
) -> Result<(), EspError> {
    info!("About to start the MQTT client");

    // Just pump the connection
    info!("MQTT Listening for messages");

    while let Ok(event) = connection.next().await {
        info!("[Queue] Event: {}", event.payload());
    }

    info!("Connection closed");

    Ok(())
}

fn mqtt_create(
    url: &str,
    client_id: &str,
    password: &str,
) -> Result<(EspAsyncMqttClient, EspAsyncMqttConnection), EspError> {
    let (mqtt_client, mqtt_conn) = EspAsyncMqttClient::new(
        url,
        &MqttClientConfiguration {
            client_id: Some(client_id),
            username: Some("bblp"),
            password: Some(password),
            skip_cert_common_name_check: true,
            server_certificate: None,
            use_global_ca_store: false,
            ..Default::default()
        },
    )?;

    Ok((mqtt_client, mqtt_conn))
}

pub async fn wifi_create(
    ssid: &str,
    pass: &str,
    modem: impl esp_idf_svc::hal::peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
    timer_service: &EspTimerService<Task>,
) -> Result<Box<EspWifi<'static>>, EspError> {
    let mut auth_method = AuthMethod::WPA2Personal;
    if ssid.is_empty() {
        log::error!("Missing WiFi name");
        return Err(EspError::from(ESP_ERR_INVALID_ARG).unwrap());
    }
    if pass.is_empty() {
        auth_method = AuthMethod::None;
        log::info!("Wifi password is empty");
    }
    let nvs = EspDefaultNvsPartition::take().ok();
    let mut esp_wifi = EspWifi::new(modem, sysloop.clone(), nvs.clone())?;

    let mut wifi = AsyncWifi::wrap(&mut esp_wifi,
                                   sysloop.clone(), timer_service.clone())?;

    wifi.set_configuration(&esp_idf_svc::wifi::Configuration::Client(
        ClientConfiguration::default(),
    ))?;

    log::info!("Starting wifi...");

    wifi.start().await?;

    log::info!("Scanning...");

    let ap_infos = wifi.scan().await?;

    let ours = ap_infos.into_iter().find(|a| a.ssid == ssid);

    let channel = if let Some(ours) = ours {
        log::info!(
            "Found configured access point {} on channel {}",
            ssid,
            ours.channel
        );
        Some(ours.channel)
    } else {
        log::info!(
            "Configured access point {} not found during scanning, will go with unknown channel",
            ssid
        );
        None
    };

    wifi.set_configuration(&esp_idf_svc::wifi::Configuration::Client(
        ClientConfiguration {
            ssid: ssid
                .try_into()
                .expect("Could not parse the given SSID into WiFi config"),
            password: pass
                .try_into()
                .expect("Could not parse the given password into WiFi config"),
            channel,
            auth_method,
            ..Default::default()
        },
    ))?;

    log::info!("Connecting wifi...");

    wifi.connect().await?;

    log::info!("Waiting for DHCP lease...");

    wifi.wait_netif_up().await?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;

    log::info!("Wifi DHCP info: {:?}", ip_info);

    Ok(Box::new(esp_wifi))
}