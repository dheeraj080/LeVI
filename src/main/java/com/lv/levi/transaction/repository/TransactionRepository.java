package com.lv.levi.transaction.repository;

import com.lv.levi.auth.entity.User;
import com.lv.levi.category.entity.Category;
import com.lv.levi.transaction.entity.Transaction;
import com.lv.levi.transaction.entity.TransactionType;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;
import org.springframework.stereotype.Repository;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.List;
import java.util.Optional;
import java.util.UUID;

@Repository
public interface TransactionRepository extends JpaRepository<Transaction, UUID> {

    // 1. Find by user and sort by date
    List<Transaction> findAllByUserOrderByTransactionDateDesc(User user);


    // 2. Find transactions for a time period
    List<Transaction> findAllByUserAndTransactionDateBetweenOrderByTransactionDateDesc(
            User user, LocalDate startDate, LocalDate endDate);

    // 3. Updated: Removed "AND t.deleted = false"
    @Query("SELECT SUM(t.amount) FROM Transaction t " +
            "WHERE t.user = :user AND t.type = :type")
    BigDecimal sumTotalByType(@Param("user") User user, @Param("type") TransactionType type);

    // 4. Updated: Removed "AND t.deleted = false"
    @Query("SELECT SUM(t.amount) FROM Transaction t " +
            "WHERE t.user = :user " +
            "AND t.category.id = :categoryId " +
            "AND t.type = :type")
    BigDecimal sumByCategoryAndType(
            @Param("user") User user,
            @Param("categoryId") UUID categoryId,
            @Param("type") TransactionType type);
}