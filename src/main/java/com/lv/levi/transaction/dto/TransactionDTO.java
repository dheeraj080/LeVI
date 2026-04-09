package com.lv.levi.transaction.dto;

import com.lv.levi.transaction.entity.TransactionType;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Positive;
import lombok.*;

import java.math.BigDecimal;
import java.time.Instant;
import java.time.LocalDate;
import java.util.UUID;

@Data
@Builder
@NoArgsConstructor
@AllArgsConstructor
public class TransactionDTO {

    private UUID id;

    @NotBlank(message = "Title is required")
    private String title;

    private String description;

    @NotNull(message = "Transaction date is required")
    private LocalDate transactionDate;

    @NotNull(message = "Amount is required")
    @Positive(message = "Amount must be greater than zero")
    private BigDecimal amount;

    @NotNull(message = "Transaction type (CREDIT/DEBIT) is required")
    private TransactionType type;

    // We pass the UUIDs of the related entities rather than the whole objects
    @NotNull(message = "Category ID is required")
    private UUID categoryId;

    // Optional: Include the category name/icon for the UI to display immediately
    private String categoryName;
    private String categoryIcon;

    private Instant createdAt;
    private Instant updatedAt;
}