package me.go_gradually.omypic.bootstrap;

import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.boot.context.properties.EnableConfigurationProperties;
import org.springframework.context.annotation.Configuration;

@Configuration
@EnableConfigurationProperties(AppProperties.class)
public class AppConfig {
}
