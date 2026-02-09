package me.go_gradually.omypic.presentation.apikey.dto;

import jakarta.validation.constraints.NotBlank;

public class ApiKeyVerifyRequest {
    @NotBlank
    private String provider;
    @NotBlank
    private String apiKey;
    private String model;

    public String getProvider() {
        return provider;
    }

    public void setProvider(String provider) {
        this.provider = provider;
    }

    public String getApiKey() {
        return apiKey;
    }

    public void setApiKey(String apiKey) {
        this.apiKey = apiKey;
    }

    public String getModel() {
        return model;
    }

    public void setModel(String model) {
        this.model = model;
    }
}
