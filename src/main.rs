use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareResponse<T> {
    success: bool,
    errors: Vec<CloudflareError>,
    #[serde(default)]
    result: Vec<T>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareError {
    code: i32,
    message: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Zone {
    id: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DnsRecord {
    id: String,
    #[serde(rename = "type")]
    record_type: String,
    name: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct DnsUpdate {
    #[serde(rename = "type")]
    record_type: String,
    name: String,
    content: String,
    proxied: bool,
}

struct Config {
    auth_email: String,
    auth_key: String,
    domain: String,
}

impl Config {
    fn from_env() -> Result<Self, Box<dyn Error>> {
        Ok(Config {
            auth_email: env::var("CF_AUTH_EMAIL")?,
            auth_key: env::var("CF_API_TOKEN")?,
            domain: env::var("CF_DOMAIN")?,
        })
    }
}

struct CloudflareClient {
    auth_email: String,
    auth_key: String,
}

impl CloudflareClient {
    fn new(auth_email: String, auth_key: String) -> Self {
        Self {
            auth_email,
            auth_key,
        }
    }

    fn get<T: for<'de> Deserialize<'de> + std::default::Default>(
        &self,
        url: &str,
    ) -> Result<CloudflareResponse<T>, Box<dyn Error>> {
        let response = ureq::get(url)
            .set("X-Auth-Email", &self.auth_email)
            .set("X-Auth-Key", &self.auth_key)
            .call()?
            .into_json()?;
        Ok(response)
    }

    fn put<T: for<'de> Deserialize<'de> + std::default::Default>(
        &self,
        url: &str,
        json: &impl Serialize,
    ) -> Result<CloudflareResponse<T>, Box<dyn Error>> {
        let response = ureq::put(url)
            .set("X-Auth-Email", &self.auth_email)
            .set("X-Auth-Key", &self.auth_key)
            .set("Content-Type", "application/json")
            .send_json(json)?
            .into_json()?;
        Ok(response)
    }

    fn get_zone_id(&self, domain: &str) -> Result<String, Box<dyn Error>> {
        let base_domain = domain
            .split('.')
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(".");

        let url = format!(
            "https://api.cloudflare.com/client/v4/zones?name={}",
            base_domain
        );

        let response: CloudflareResponse<Zone> = self.get(&url)?;

        response
            .result
            .first()
            .map(|zone| zone.id.clone())
            .ok_or_else(|| "Zone not found".into())
    }

    fn get_dns_records(
        &self,
        zone_id: &str,
        domain: &str,
    ) -> Result<(Option<String>, Option<String>), Box<dyn Error>> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records?name={}",
            zone_id, domain
        );

        let response: CloudflareResponse<DnsRecord> = self.get(&url)?;

        let mut ipv4_id = None;
        let mut ipv6_id = None;

        for record in response.result {
            match record.record_type.as_str() {
                "A" => ipv4_id = Some(record.id),
                "AAAA" => ipv6_id = Some(record.id),
                _ => {}
            }
        }

        Ok((ipv4_id, ipv6_id))
    }

    fn update_dns(
        &self,
        zone_id: &str,
        record_id: &str,
        domain: &str,
        ip: &str,
        record_type: &str,
    ) -> Result<(), Box<dyn Error>> {
        let update = DnsUpdate {
            record_type: record_type.to_string(),
            name: domain.to_string(),
            content: ip.to_string(),
            proxied: false,
        };

        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
            zone_id, record_id
        );

        let response: CloudflareResponse<DnsRecord> = self.put(&url, &update)?;

        if response.success {
            println!("{} record updated successfully to {}", record_type, ip);
        } else {
            println!("{} update failed: {:?}", record_type, response.errors);
        }

        Ok(())
    }
}

fn get_ip_from_trace() -> Result<(Option<String>, Option<String>), Box<dyn Error>> {
    let response = ureq::get("https://cloudflare.com/cdn-cgi/trace")
        .call()?
        .into_string()?;

    let mut ipv4 = None;
    let mut ipv6 = None;

    for line in response.lines() {
        if line.starts_with("ip=") {
            let ip = line.trim_start_matches("ip=");
            if ip.contains(':') {
                ipv6 = Some(ip.to_string());
            } else {
                ipv4 = Some(ip.to_string());
            }
        }
    }

    Ok((ipv4, ipv6))
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    let client = CloudflareClient::new(config.auth_email, config.auth_key);

    println!("Starting DNS update for {}", config.domain);

    // Discover zone ID
    println!("Discovering zone ID...");
    let zone_id = client.get_zone_id(&config.domain)?;
    println!("Found zone ID: {}", zone_id);

    // Discover record IDs
    println!("Discovering DNS record IDs...");
    let (ipv4_id, ipv6_id) = client.get_dns_records(&zone_id, &config.domain)?;

    println!(
        "Found record IDs - IPv4: {:?}, IPv6: {:?}",
        ipv4_id, ipv6_id
    );

    // Get current IP addresses from Cloudflare trace
    let (ipv4, ipv6) = get_ip_from_trace()?;

    // Update IPv4 record if available
    if let (Some(ipv4), Some(ipv4_id)) = (ipv4, ipv4_id) {
        client.update_dns(&zone_id, &ipv4_id, &config.domain, &ipv4, "A")?;
    } else {
        println!("Skipping IPv4 update - address or record not available");
    }

    // Update IPv6 record if available
    if let (Some(ipv6), Some(ipv6_id)) = (ipv6, ipv6_id) {
        client.update_dns(&zone_id, &ipv6_id, &config.domain, &ipv6, "AAAA")?;
    } else {
        println!("Skipping IPv6 update - address or record not available");
    }

    Ok(())
}
