# Current script shortfalls:

- No mechanism to notify administrator on failure
  - Especially considering it's on a different system to the administrator
- Should check if the IP address is valid
  - I suspect this is what's causing the errors
  - Log output from icanhazip, but only when it fails?

# Program parts:

- Network:
  - Cloudflare GET zone and record
  - Cloudflare PUT vpn ip address change
  - Cloudflare PUT access ip address change
  - icanhazip GET ip address
- File system:
  - SET zone and record
  - GET zone and record
  - GET hist IP
  - SET ip
  - Log errors
    - Cloudflare response
    - Icanhazip response
    - Zone and Record ID
    - Hist IP
    - Current IP
  - Log successful change
- Logic
  - Compare hist IP and current IP
  - Check validity of IP from icanhazip
  - Check Cloudflare PUT response x1
  - Check Cloudflare PUT response x2

# Components:

- Curl interface
- Response filtering
- Filesystem interface
  - Log interface
-
