-- P8 desktop sign-in: the site creates a devices row when the app exchanges its
-- one-time code for a device token. Grants only; no table changes.
grant insert on devices to convt_web;
