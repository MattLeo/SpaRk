import { userState, useRef, useEffect } from 'react';
import './MessageInput.css';

function MessageInput ({
    onSendMessage,
    replyingTo = null,
    onCancelReply = null,
    mentionSuggestions = [],
    showMentionMenu = false,
    onInputChange = null,
    initialValue = ''
}) {
    const [MessageInput, setMessageInput] = userState(initialValue);
    const [isExpanded, seetIsExpanded] = useState(false);
    const inputRef = useRef(null);
    const textareaRef = useRef(null);

    useEffect(() => {
        setMessageInput(initialValue);
    }, [initialValue]);

    const handleSend = (e) => {
        e?.preventDefault();

        if (MessageInput.trim()) {
            const contentFormat = isExpanded ? 'MARKDOWN': 'TEXT';
            onSendMessage(MessageInput, contentFormat);
            setMessageInput('');

            if (isExpanded && textareaRef.current) {
                textareaRef.current.focus();
            } else if (inputRef.current) {
                inputRef.current.focus();
            }
        }
    };

    const handleInputChange = (e) => {
        const value = e.target.value;
        setMessageInput(value);

        if (onInputChange) {
            onInputChange(e);
        }
    };

    const handleKeyDown = (e) => {
        if (isExpanded) {
            if (e.key === 'Enter' && e.shiftKey) {
                e.preventDefault();
                handleSend();
            }
        } else {
            if (e.key === 'Enter') {
                e.preventDefault();
                handleSend();
            }
        }
    };

    const toggleExpanded = () => {
        seetIsExpanded(!isExpanded);

        setTimeout(() => {
            if (!isExpanded && textareaRef.current) {
                textareaRef.current.focus();
            } else if (isExpanded && textareaRef.current) {
                inputRef.current.focus();
            }
        }, 100);
    };

    const insertMarkdown = (before, after = '') => {
        const textarea = textareaRef.current;
        if (!textarea) return;

        const start = textarea.selectionStart;
        const end = textarea.selectionEnd;
        const selectedText = MessageInput.substring(start, end);
        const newText = 
            MessageInput.substring(0, start) +
            before +
            selectedText +
            after +
            MessageInput.substring(end);

        setMessageInput(newText);

        setTimeout(() => {
            const newCursorPos = selectedText
            ? start + before.length + selectedText.length + after.length
            : start + before.length;
            textarea.setSelectionRange(newCursorPos, newCursorPos);
            textarea.focus();
        }, 0)
    };

    const formatButtons = [
        { label: 'B', title: 'Bold (Ctrl + B)', action: () => insertMarkdown('**', '**') },
        { label: 'I', title: 'Italic (Ctrl + I)', action: () => insertMarkdown('*', '*')  },
        { label: 'S', title: 'Strikethrough', action: () => insertMarkdown('~~', '~~') },
        { label: '</>', title: 'Code', action: () => insertMarkdown('`', '`') },
        { label: '[]', title: 'Link', action: () => insertMarkdown('[', '](url)') },
        { label: '•', title: 'Billet List', action: () => insertMarkdown('\n-', '') },
        { label: '1.', title: 'Numbered List', action: () => insertMarkdown('\n1. ', '') },
        { label: '```', title: 'Code Block', action: () => insertMarkdown('\n```\n', '\n```\n') },
    ];

    useEffect(() => {
        const handleShortcuts = (e) => {
            if (!isExpanded) return;

            if (e.ctrlKey || e.metaKey) {
                switch (e.key.toLowerCase()) {
                    case 'b':
                        e.preventDefault();
                        insertMarkdown('**', '**');
                        break;
                    case '1':
                        e.preventDefault();
                        insertMarkdown('*', '*');
                        break;
                    default:
                        break;
                }
            }
        };

        if (isExpanded && textareaRef.current) {
            textareaRef.current.addEventListener('keydown', handleShortcuts);
            return () => {
                if (textareaRef.current) {
                    textareaRef.current.removeEventListener('keydown', handleShortcuts);
                }
            };
        }
    }, [isExpanded, MessageInput]);

    return (
        <div className='chat-input-container'>
            {replyingTo && (
                <div className='reply-preview'>
                    <div className='reply-preview-content'>
                        <span className='reply-preview-label'>
                            Replying to {replyingTo.senderUsername}
                        </span>
                        <span className='reply-preview-text'>
                            {replyingTo.content.length > 100
                                ? replyingTo.content.substring(0, 100) + '...'
                                : replyingTo.content
                            }
                        </span>
                    </div>
                    <button
                        className='reply-preview-cancel'
                        onClick={onCancelReply}
                        title='Cancel reply'
                    >x</button>
                </div>
            )}

            {showMentionMenu && mentionSuggestions.length > 0 && (
                <div className='mention-menu'>
                    {mentionSuggestions.map((suggestions, idx) => (
                        <div
                            key={idx}
                            className='mention-suggestion'
                            onClick={() => {

                            }}
                        >
                            {suggestion == 'everyone' ? (
                                <span className='mention-everyone-suggestion'>📢 @everyone</span>
                            ) : (
                                <span>@{suggestion}</span>
                            )}
                        </div>
                    ))}
                </div>
            )}

            <form onSubmit={handleSend} className='message-input-form'>
                <div className={`input-wrapper ${isExpanded ? 'expanded' : ''}`}>
                    {!isExpanded ? (
                        <input
                            ref={inputRef}
                            type='text'
                            value={MessageInput}
                            onChange={handleInputChange}
                            onKeyDown={handleKeyDown}
                            placeholder='Type a message...'
                            className='message-input'
                        />
                    ) : (
                        <div className='expanded-input-container'>
                            <div className='formatting-toolbar'>
                                {formatButtons.map((btn, idx) => (
                                    <button
                                        key={idx}
                                        type='button'
                                        className='format-btn'
                                        onClick={btn.action}
                                        title={btn.title}
                                    >
                                        {btn.label}
                                    </button>
                                ))}
                            </div>
                            <textarea
                                ref={textareaRef}
                                value={MessageInput}
                                onChange={handleInputChange}
                                onKeyDown={handleKeyDown}
                                placeholder='Type a message... (Shift + Enter to send)'
                                className='message-textarea'
                                rows={5}
                            />
                        </div>
                    )}

                    <button
                        type='button'
                        className='expand-btn'
                        onClick={}

                </div>
            </form>
        </div>
    );
}