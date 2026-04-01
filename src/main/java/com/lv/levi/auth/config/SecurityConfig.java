package com.lv.levi.auth.config;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.lv.levi.auth.security.CustomUserDetailService;
import com.lv.levi.auth.security.JwtAuthenticationFilter;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.security.authentication.AuthenticationManager;
import org.springframework.security.authentication.dao.DaoAuthenticationProvider;
import org.springframework.security.config.Customizer;
import org.springframework.security.config.annotation.authentication.configuration.AuthenticationConfiguration;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.config.annotation.web.configuration.EnableWebSecurity;
import org.springframework.security.config.annotation.web.configurers.AbstractHttpConfigurer;
import org.springframework.security.config.annotation.web.configurers.oauth2.client.OAuth2LoginConfigurer;
import org.springframework.security.config.http.SessionCreationPolicy;
import org.springframework.security.crypto.bcrypt.BCryptPasswordEncoder;
import org.springframework.security.crypto.password.PasswordEncoder;
import org.springframework.security.web.SecurityFilterChain;
import org.springframework.security.web.authentication.AuthenticationSuccessHandler;
import org.springframework.security.web.authentication.UsernamePasswordAuthenticationFilter;

import java.util.Map;

@Configuration
@EnableWebSecurity
public class SecurityConfig {


    private JwtAuthenticationFilter jwtAuthenticationFilter;
    private AuthenticationSuccessHandler authenticationSuccessHandler;

    public SecurityConfig(JwtAuthenticationFilter jwtAuthenticationFilter, AuthenticationSuccessHandler authenticationSuccessHandler) {
        this.jwtAuthenticationFilter = jwtAuthenticationFilter;
        this.authenticationSuccessHandler = authenticationSuccessHandler;
    }

    @Bean
    public SecurityFilterChain securityFilterChain(HttpSecurity http) throws Exception {
        http
                .csrf(AbstractHttpConfigurer::disable)
                .cors(Customizer.withDefaults())
                // OAuth2 needs session to store the state parameter during the redirect flow
                .sessionManagement(sm -> sm.sessionCreationPolicy(SessionCreationPolicy.IF_REQUIRED))
                .authorizeHttpRequests(auth -> auth
                        .requestMatchers("/api/v1/auth/**", "/login/**", "/error").permitAll()
                        .requestMatchers("/v3/api-docs/**", "/swagger-ui/**", "/swagger-ui.html").permitAll()
                        .requestMatchers("/api/public").permitAll()
                        // ADD THIS LINE BEFORE anyRequest()
                        .requestMatchers("/api/admin/**").hasRole("ADMIN")
                        .anyRequest().authenticated()
                )
                .oauth2Login(oauth2 -> oauth2
                        .successHandler(authenticationSuccessHandler)
                        .failureHandler((request, response, exception) -> {
                            // Log the real error so you can see what's happening
                            exception.printStackTrace();
                            response.setStatus(401);
                            response.setContentType("application/json");
                            ObjectMapper mapper = new ObjectMapper();
                            response.getWriter().write(mapper.writeValueAsString(
                                    Map.of("statusCode", "401", "message", exception.getMessage())
                            ));
                        })
                )
                .logout(AbstractHttpConfigurer::disable)
                .exceptionHandling(ex -> ex.authenticationEntryPoint((request, response, authException) -> {
                    authException.printStackTrace();
                    response.setStatus(401);
                    response.setContentType("application/json");
                    ObjectMapper mapper = new ObjectMapper();
                    response.getWriter().write(mapper.writeValueAsString(Map.of(
                            "statusCode", "401",
                            "message", "Unauthorized: " + authException.getMessage()
                    )));
                }))
                .addFilterBefore(jwtAuthenticationFilter, UsernamePasswordAuthenticationFilter.class);

        return http.build();
    }

    @Bean
    public PasswordEncoder passwordEncoder() {
        return new BCryptPasswordEncoder();
    }

    @Bean
    public AuthenticationManager authenticationManagerBean(AuthenticationConfiguration configuration) {
        return configuration.getAuthenticationManager();
    }

    @Bean
    public DaoAuthenticationProvider authenticationProvider(
            CustomUserDetailService userDetailsService,
            PasswordEncoder passwordEncoder) {

        // Pass userDetailsService directly into the constructor
        DaoAuthenticationProvider authProvider = new DaoAuthenticationProvider(userDetailsService);

        authProvider.setPasswordEncoder(passwordEncoder);
        authProvider.setHideUserNotFoundExceptions(false);
        return authProvider;
    }

}