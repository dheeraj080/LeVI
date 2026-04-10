package com.lv.levi.auth.security;

import com.lv.levi.auth.UserPrincipal;
import com.lv.levi.auth.repository.UserRepository;
import io.jsonwebtoken.Claims;
import io.jsonwebtoken.ExpiredJwtException;
import io.jsonwebtoken.Jws;
import io.jsonwebtoken.JwtException;
import jakarta.servlet.FilterChain;
import jakarta.servlet.ServletException;
import jakarta.servlet.http.HttpServletRequest;
import jakarta.servlet.http.HttpServletResponse;
import lombok.NonNull;
import lombok.RequiredArgsConstructor;
import lombok.extern.slf4j.Slf4j;
import org.springframework.http.MediaType;
import org.springframework.security.authentication.BadCredentialsException;
import org.springframework.security.authentication.UsernamePasswordAuthenticationToken;
import org.springframework.security.core.context.SecurityContextHolder;
import org.springframework.security.web.authentication.WebAuthenticationDetailsSource;
import org.springframework.stereotype.Component;
import org.springframework.web.filter.OncePerRequestFilter;

import java.io.IOException;
import java.util.UUID;

@Slf4j
@Component
@RequiredArgsConstructor
public class JwtAuthenticationFilter extends OncePerRequestFilter {

    private final JwtService jwtService;
    private final UserRepository userRepository;

    @Override
    protected void doFilterInternal(
            @NonNull HttpServletRequest request,
            @NonNull HttpServletResponse response,
            @NonNull FilterChain filterChain) throws ServletException, IOException {

        final String authHeader = request.getHeader("Authorization");
        log.info("authHeader: {}", authHeader);

        if (authHeader == null || !authHeader.startsWith("Bearer ")) {
            filterChain.doFilter(request, response);
            return;
        }

        final String token = authHeader.substring(7);

        try {
            if (!jwtService.isAccessToken(token)) {
                throw new BadCredentialsException("Invalid token type");
            }

            Jws<Claims> parsed = jwtService.parse(token);
            String userId = jwtService.getUserId(token).toString();

            if (SecurityContextHolder.getContext().getAuthentication() == null) {
                userRepository.findById(UUID.fromString(userId)).ifPresent(user -> {
                    if (user.isEnabled() && user.isAccountNonLocked()) {

                        // NEW: Create the public principal instead of passing the entity
                        UserPrincipal principal = new UserPrincipal(user.getId(), user.getEmail());

                        UsernamePasswordAuthenticationToken auth =
                                new UsernamePasswordAuthenticationToken(
                                        principal, // Use the Record here
                                        null,
                                        user.getAuthorities());

                        auth.setDetails(new WebAuthenticationDetailsSource().buildDetails(request));
                        SecurityContextHolder.getContext().setAuthentication(auth);
                    }
                });
            }

            filterChain.doFilter(request, response);

        } catch (ExpiredJwtException e) {
            log.error("JWT Token expired: {}", e.getMessage());
            sendError(response, HttpServletResponse.SC_UNAUTHORIZED, "Token expired");
        } catch (JwtException | BadCredentialsException | IllegalArgumentException e) {
            log.error("JWT Validation failed: {}", e.getMessage());
            SecurityContextHolder.clearContext();
            sendError(response, HttpServletResponse.SC_UNAUTHORIZED, "Invalid token");
        } catch (Exception e) {
            log.error("Unexpected error in security filter: ", e);
            sendError(response, HttpServletResponse.SC_INTERNAL_SERVER_ERROR, "Internal server error");
        }
    }

    @Override
    protected boolean shouldNotFilter(HttpServletRequest request) {
        return request.getRequestURI().startsWith("/api/v1/auth");
    }

    private void sendError(HttpServletResponse response, int status, String message)
            throws IOException {
        response.setStatus(status);
        response.setContentType(MediaType.APPLICATION_JSON_VALUE);
        response.getWriter().write("""
                {"message": "%s", "status": %d}
                """.formatted(message, status));
    }
}