package com.lv.levi.transaction.repository;

import com.lv.levi.transaction.entity.Transaction;
import com.lv.levi.transaction.entity.TransactionType;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;
import org.springframework.stereotype.Repository;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.List;
import java.util.UUID;

@Repository
public interface TransactionRepository extends JpaRepository<Transaction, UUID> {

    // 1. FIX: Changed 'User' to 'UserId' to match the field name in the Entity
    List<Transaction> findAllByUserIdOrderByTransactionDateDesc(UUID userId);

    // 2. FIX: Changed 'User' to 'UserId'
    List<Transaction> findAllByUserIdAndTransactionDateBetweenOrderByTransactionDateDesc(
            UUID userId, LocalDate startDate, LocalDate endDate);

    // 3. FIX: Query updated to use 't.userId'
    @Query("SELECT SUM(t.amount) FROM Transaction t " +
            "WHERE t.userId = :userId AND t.type = :type")
    BigDecimal sumTotalByType(@Param("userId") UUID userId, @Param("type") TransactionType type);

    // 4. FIX: Query updated to use 't.userId' and 't.categoryId'
    // Note: We use t.categoryId directly because it's now a basic UUID field
    @Query("SELECT SUM(t.amount) FROM Transaction t " +
            "WHERE t.userId = :userId " +
            "AND t.categoryId = :categoryId " +
            "AND t.type = :type")
    BigDecimal sumByCategoryAndType(
            @Param("userId") UUID userId,
            @Param("categoryId") UUID categoryId,
            @Param("type") TransactionType type);
}