package me.go_gradually.omypic.application.apikey.model;

import java.time.Instant;

public class ApiKeyVerifyResult {
    private boolean valid;
    private String provider;
    private Instant checkedAt;
    private String message;

    public static ApiKeyVerifyResult success(String provider) {
        ApiKeyVerifyResult result = new ApiKeyVerifyResult();
        result.setValid(true);
        result.setProvider(provider);
        result.setCheckedAt(Instant.now());
        result.setMessage("Verified");
        return result;
    }

    public static ApiKeyVerifyResult failure(String provider, String message) {
        ApiKeyVerifyResult result = new ApiKeyVerifyResult();
        result.setValid(false);
        result.setProvider(provider);
        result.setCheckedAt(Instant.now());
        result.setMessage(message);
        return result;
    }

    public boolean isValid() {
        return valid;
    }

    public void setValid(boolean valid) {
        this.valid = valid;
    }

    public String getProvider() {
        return provider;
    }

    public void setProvider(String provider) {
        this.provider = provider;
    }

    public Instant getCheckedAt() {
        return checkedAt;
    }

    public void setCheckedAt(Instant checkedAt) {
        this.checkedAt = checkedAt;
    }

    public String getMessage() {
        return message;
    }

    public void setMessage(String message) {
        this.message = message;
    }
}
