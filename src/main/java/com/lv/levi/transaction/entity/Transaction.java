package com.lv.levi.transaction.entity;

import com.lv.levi.auth.entity.User;
import com.lv.levi.category.entity.Category;
import jakarta.persistence.*;
import lombok.*;

import java.math.BigDecimal;
import java.time.Instant;
import java.time.LocalDate;
import java.util.UUID;

@AllArgsConstructor
@NoArgsConstructor
@Getter
@Setter
@Entity
@Builder
@Table(name = "transactions")
public class Transaction { // Renamed from "Transactions" (Standard practice is singular)

    @Id
    @GeneratedValue(strategy = GenerationType.UUID)
    @Column(name = "transaction_id")
    private UUID id;

    @Column(nullable = false)
    private String title;

    private String description;

    private LocalDate transactionDate;

    @Column(nullable = false)
    private BigDecimal amount;

    @Enumerated(EnumType.STRING)
    private TransactionType type;

    @Builder.Default
    @Column(updatable = false, nullable = false)
    private Instant createdAt = Instant.now();

    @Builder.Default
    private Instant updatedAt = Instant.now();

    // FIXED: Points to Category Entity, not the Enum
    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(
            name = "category_id",
            referencedColumnName = "category_id", // Points to Category PK
            nullable = false
    )
    private Category category;

    // FIXED: Changed name to 'user_id' to avoid conflict with 'transaction_id'
    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(
            name = "user_id",
            referencedColumnName = "user_id",
            nullable = false
    )
    private User user;

    @PrePersist // Added annotation
    public void prePersist() {
        if (this.transactionDate == null) {
            this.transactionDate = LocalDate.now();
        }
    }
}
