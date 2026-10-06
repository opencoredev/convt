-- Reverses 0002_device_auth. Local development only: production is forward-only.
revoke insert on devices from convt_web;
