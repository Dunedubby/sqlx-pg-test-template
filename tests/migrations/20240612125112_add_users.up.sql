BEGIN;

CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,

    -- user data
    phone VARCHAR(255) UNIQUE
        CONSTRAINT "phone must follow E.164 format" CHECK (phone IS NULL OR phone ~ '^[+][1-9][0-9]{1,14}$'),
    first_name VARCHAR(255),
    last_name VARCHAR(255),
    email VARCHAR(255)
        CONSTRAINT "email must be valid" CHECK (email IS NULL or email ~ '^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+[.][a-zA-Z]{2,}$'),

    -- verification data
    phone_to_verify VARCHAR(255),
    code_to_verify VARCHAR(20),
    code_to_verify_expires_at TIMESTAMP,
    verification_locked_until TIMESTAMP,
    verified_at TIMESTAMP
);

-- care about consistency
CREATE OR REPLACE FUNCTION clean_user_data() RETURNS TRIGGER AS $$
    BEGIN
        IF NEW != OLD THEN
            NEW.updated_at = NOW();
        END IF;

        IF OLD.created_at IS NOT NULL AND NEW.created_at != OLD.created_at THEN
            NEW.created_at = OLD.created_at;
        END IF;

        IF NEW.code_to_verify IS NULL
            OR NEW.phone_to_verify IS NULL
            OR NEW.code_to_verify_expires_at IS NULL
            OR NEW.code_to_verify_expires_at < NOW()
        THEN
            NEW.code_to_verify = NULL;
            NEW.code_to_verify_expires_at = NULL;
        END IF;

        RETURN NEW;
    END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER clean_user_data
    BEFORE INSERT OR UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION clean_user_data();

-- ensure uniqueness of phones across both columns

CREATE FUNCTION min_phone(users) RETURNS VARCHAR(255) AS $$
    SELECT CASE
        WHEN $1.phone IS NULL THEN NULL
        WHEN $1.phone_to_verify IS NULL THEN $1.phone
        WHEN $1.phone <= $1.phone_to_verify THEN $1.phone
        ELSE $1.phone_to_verify
    END
$$ LANGUAGE sql;

CREATE FUNCTION max_phone(users) RETURNS VARCHAR(255) AS $$
    SELECT CASE
        WHEN $1.phone IS NULL THEN NULL
        WHEN $1.phone_to_verify IS NULL THEN $1.phone
        WHEN $1.phone > $1.phone_to_verify THEN $1.phone
        ELSE $1.phone_to_verify
    END
$$ LANGUAGE sql;

CREATE UNIQUE INDEX users_min_phone_idx ON users (min_phone(users)) WHERE phone IS NOT NULL;

CREATE UNIQUE INDEX users_max_phone_idx ON users (max_phone(users)) WHERE phone IS NOT NULL;

-- find user for code verification
CREATE UNIQUE INDEX users_verification_idx ON users (phone_to_verify) WHERE phone_to_verify IS NOT NULL;

-- find user for authentication by access token
CREATE UNIQUE INDEX users_authentication_idx ON users (
    id,
    phone,
    verified_at
) WHERE phone IS NOT NULL AND verified_at IS NOT NULL;

COMMIT;
