package com.lv.levi.auth.repository;

import com.lv.levi.auth.entity.ResetPasswordToken;
import com.lv.levi.auth.entity.User;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Modifying;
import org.springframework.data.jpa.repository.Query;
import org.springframework.transaction.annotation.Transactional;

import java.util.Optional;
import java.util.UUID;

public interface ResetPasswordTokenRepository extends JpaRepository<ResetPasswordToken, UUID> {
    // Look for the code AND the specific User entity
    Optional<ResetPasswordToken> findByCodeAndUser(String code, User user);

    @Modifying
    @Transactional
    @Query("DELETE FROM ResetPasswordToken r WHERE r.user = :user")
    void deleteByUser(User user);
}