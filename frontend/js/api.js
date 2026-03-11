export function isAddressLike(value) {
    const evmLike = /^0x[a-fA-F0-9]{40}$/;
    const base58Like = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;
    return evmLike.test(value) || base58Like.test(value);
}

export function validateSolanaAddress(value) {
    if (value.startsWith('0x') || value.startsWith('0X')) {
        return { isValid: false, message: 'Invalid address type: EVM (0x...) addresses are not supported' };
    }
    if (value.length < 32 || value.length > 44) {
        return { isValid: false, message: 'Invalid Solana address length (allowed: 32-44 chars)' };
    }
    if (/[0OIl]/.test(value)) {
        return { isValid: false, message: 'Invalid Solana address: contains forbidden characters (0, O, I, l)' };
    }
    if (!/^[1-9A-HJ-NP-Za-km-z]+$/.test(value)) {
        return { isValid: false, message: 'Invalid Solana address format' };
    }
    return { isValid: true, message: '' };
}

export async function submitAnalyzeRequest(address, filters) {
    const response = await fetch('http://127.0.0.1:8080/analyze', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            address,
            requested_hours: filters.time,
            txLimit: filters.txLimit
        }),
    });

    if (!response.ok) {
        const contentType = response.headers.get('content-type') || '';
        let errorMessage = 'Failed to start indexing';

        if (contentType.includes('application/json')) {
            const errorData = await response.json().catch(() => ({}));
            errorMessage = errorData.message || errorData.error || errorMessage;
        } else {
            const errorText = await response.text().catch(() => '');
            if (errorText) {
                errorMessage = errorText;
            }
        }
        throw new Error(errorMessage);
    }

    return await response.json();
}

export async function fetchJobInfo(jobId) {
    const response = await fetch(`http://127.0.0.1:8080/jobs/${jobId}`);

    if (!response.ok) {
        const contentType = response.headers.get('content-type') || '';
        let errorMessage = 'Failed to fetch job info';

        if (contentType.includes('application/json')) {
            const errorData = await response.json().catch(() => ({}));
            errorMessage = errorData.message || errorData.error || errorMessage;
        } else {
            const errorText = await response.text().catch(() => '');
            if (errorText) {
                errorMessage = errorText;
            }
        }

        throw new Error(errorMessage);
    }

    return await response.json();
}
