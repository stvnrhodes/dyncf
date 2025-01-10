package main

import (
	"bufio"
	"context"
	"flag"
	"fmt"
	"log"
	"log/slog"
	"net"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/libdns/cloudflare"
	"github.com/libdns/libdns"
)

func getMyIP(recordType string) (net.IP, error) {
	var netType string
	switch recordType {
	case "A":
		netType = "tcp4"
	case "AAAA":
		netType = "tcp6"
	default:
		return nil, fmt.Errorf("unknown record type %v", recordType)
	}
	client := &http.Client{
		Transport: &http.Transport{
			DialContext: func(ctx context.Context, network string, addr string) (net.Conn, error) {
				return (&net.Dialer{}).DialContext(ctx, netType, addr)
			},
		},
	}
	resp, err := client.Get("https://cloudflare.com/cdn-cgi/trace")
	if err != nil {
		return nil, err
	}
	scanner := bufio.NewScanner(resp.Body)
	for scanner.Scan() {
		if strings.HasPrefix(scanner.Text(), "ip=") {
			return net.ParseIP(strings.TrimPrefix(scanner.Text(), "ip=")), nil
		}
	}
	return nil, fmt.Errorf("no address found")
}

func main() {
	ctx := context.Background()

	domain := flag.String("dns-domain", "", "Domain to update")
	flag.Parse()

	parts := strings.Split(*domain, ".")
	if len(parts) < 3 {
		log.Fatalf("too few domain labels in %q", *domain)
	}
	zone := strings.Join(parts[len(parts)-2:], ".")
	subdomain := strings.Join(parts[:len(parts)-2], ".")
	slog.Info("parsed domain", "zone", zone, "subdomain", subdomain)

	apiToken := os.Getenv("CLOUDFLARE_API_TOKEN")
	if apiToken == "" {
		log.Fatal("CLOUDFLARE_API_TOKEN env var is missing")
	}
	provider := cloudflare.Provider{APIToken: apiToken}

	var records []libdns.Record
	for _, recordType := range []string{"A", "AAAA"} {
		addr, err := getMyIP(recordType)
		if err != nil {
			log.Fatalf("could not get v4 address: %v", err)
		}
		records = append(records, libdns.Record{
			Type:  recordType,
			Name:  subdomain,
			Value: addr.String(),
			TTL:   5 * time.Minute,
		})
		slog.Info("will set record", "type", recordType, "value", addr)
	}

	result, err := provider.SetRecords(ctx, zone, records)
	if err != nil {
		log.Fatalf("could not update records: %v", err)
	}
	slog.Info("updated records", "records", result)
}
