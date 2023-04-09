#!/bin/dash
# Cloudflare as Dynamic DNS
# Based on: https://letswp.io/cloudflare-as-dynamic-dns-raspberry-pi/
# Which is based on: https://gist.github.com/benkulbertis/fff10759c2391b6618dd/
# Original non-RPi article: https://phillymesh.net/2016/02/23/setting-up-dynamic-dns-for-your-registered-domain-through-cloudflare/

auth_token=$(cat token.env) 
zone_name="hdert.com"
record_name="vpn.hdert.com"

ip=$(curl -s https://ipv4.icanhazip.com)
ip_file="ip.txt"
id_file="cloudflare.ids"
log_file="cloudflare.log"

current="$(pwd)"
cd "$(dirname "$(readlink -f "$0")")"

log() {
	if [ "$1" ]; then
		echo "[$(date)] - $1" >> $log_file
	fi
}

log "Check Initiated"

if [ -f $ip_file ]; then
	old_ip=$(cat $ip_file)
	if [ $ip == $old_ip ]; then
		log "IP has not changed"
		exit 0
	fi
fi

if [ -f $id_file ] && [ $(wc -l $id_file | cut -d " " -f 1) == 2 ]; then
	zone_identifier=$(head -1 $id_file)
	record_identifier=$(tail -1 $id_file)
else
	zone_identifier=$(curl -s -X GET "https://api.cloudflare.com/client/v4/zones?name=$zone_name" -H "Authorization: Bearer $auth_token" -H "Content-Type: application/json" | grep -Po '(?<="id":")[^"]*' | head -1)
	record_identifier=$(curl -s -X GET "https://api.cloudflare.com/client/v4/zones/$zone_identifier/dns_records?name=$record_name" -H "Authorization: Bearer $auth_token" -H "Content-Type: application/json" | grep -Po '(?<="id":")[^"]*')
	echo "$zone_identifier" > $id_file
	echo "$record_identifier" >> $id_file
fi

update=$(curl -s -X PUT "https://api.cloudflare.com/client/v4/zones/$zone_identifier/dns_records/$record_identifier" -H "Authorization: Bearer $auth_token" -H "Content-Type: application/json" --data "{\"id\":\"$zone_identifier\",\"type\":\"A\",\"name\":\"$record_name\",\"content\":\"$ip\"}")

if [[ $update == *"\"success\":false"* ]]; then
	message="API UPDATE FAILED. DUMPING RESULTS:\n$update"
	log "$message"
	echo "$message"
	exit 1
else
	message="IP changed to: $ip"
	echo "$ip" > $ip_file
	log "$message"
	echo "$message"
fi

