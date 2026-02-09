package me.go_gradually.omypic.application.apikey.port;

public interface ApiKeyProbePort {
    void probe(String provider, String apiKey, String model) throws Exception;
}
