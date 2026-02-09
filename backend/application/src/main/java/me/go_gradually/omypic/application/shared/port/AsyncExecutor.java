package me.go_gradually.omypic.application.shared.port;

public interface AsyncExecutor {
    void execute(Runnable task);
}
