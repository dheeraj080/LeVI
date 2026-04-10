package com.lv.levi.transaction.entity;

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
public class Transaction {

    @Id
    @GeneratedValue(strategy = GenerationType.UUID)
    @Column(name = "transaction_id")
    private UUID id;

    @Column(nullable = false)
    private String title;

    private String description;

    @Column(name = "transaction_date")
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

    // FIX 1: Use UUID to decouple from Category module internals
    @Column(name = "category_id", nullable = false)
    private UUID categoryId;

    // FIX 2: Use UUID to decouple from Auth module internals
    // Note: Removed @ManyToOne because we are only storing the ID
    @Column(name = "user_id", nullable = false)
    private UUID userId;

    @PrePersist
    public void prePersist() {
        if (this.transactionDate == null) {
            this.transactionDate = LocalDate.now();
        }
        if (this.createdAt == null) {
            this.createdAt = Instant.now();
        }
        this.updatedAt = Instant.now();
    }
}