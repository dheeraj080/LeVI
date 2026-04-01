package com.lv.levi.auth.repository;

import com.lv.levi.auth.entity.RefreshToken;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.Optional;
import java.util.UUID;

public interface RefreshTokenRepository extends JpaRepository<RefreshToken, UUID> {


    Optional<RefreshToken> findByJti(String jti);
}
